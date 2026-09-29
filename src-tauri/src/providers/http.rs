use std::{fmt, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{header::RETRY_AFTER, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const MAX_PROVIDER_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

pub async fn response_bytes(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read provider response")?
    {
        if body
            .len()
            .checked_add(chunk.len())
            .is_none_or(|length| length > limit)
        {
            return Err("Provider response exceeds the safe size limit".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDiagnostic {
    pub provider: String,
    pub endpoint: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_ref: Option<String>,
    pub attempts: usize,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProviderError {
    provider: String,
    endpoint: String,
    kind: &'static str,
    http_status: Option<u16>,
    error_type: Option<String>,
    error_code: Option<String>,
    request_id: Option<String>,
    attempts: usize,
    retryable: bool,
    retry_at: Option<DateTime<Utc>>,
}

impl ProviderError {
    pub fn network(provider: &str, endpoint: &str, attempts: usize) -> Self {
        Self {
            provider: provider.into(),
            endpoint: safe_endpoint(endpoint),
            kind: "network",
            http_status: None,
            error_type: None,
            error_code: None,
            request_id: None,
            attempts,
            retryable: true,
            retry_at: None,
        }
    }

    pub fn invalid_response(provider: &str, endpoint: &str, attempts: usize) -> Self {
        Self {
            provider: provider.into(),
            endpoint: safe_endpoint(endpoint),
            kind: "invalid_response",
            http_status: None,
            error_type: None,
            error_code: None,
            request_id: None,
            attempts,
            retryable: false,
            retry_at: None,
        }
    }

    pub async fn from_response(
        provider: &str,
        endpoint: &str,
        response: reqwest::Response,
        attempts: usize,
    ) -> Result<Value, Self> {
        let status = response.status();
        let retry_after = retry_after(&response);
        let payload = serde_json::from_slice::<Value>(
            &response_bytes(response, MAX_PROVIDER_RESPONSE_BYTES)
                .await
                .map_err(|_| Self::invalid_response(provider, endpoint, attempts))?,
        )
        .map_err(|_| Self::invalid_response(provider, endpoint, attempts))?;
        if status.is_success() {
            return Ok(payload);
        }

        let error_type = safe_metadata(payload.get("error_type"));
        let error_code = safe_metadata(payload.get("error_code"));
        let request_id = payload
            .get("request_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let institution_throttle = error_code.as_deref() == Some("INSTITUTION_RATE_LIMIT");
        let retryable = status.is_server_error()
            || status == StatusCode::TOO_MANY_REQUESTS
            || matches!(
                error_type.as_deref(),
                Some("INSTITUTION_ERROR" | "RATE_LIMIT_EXCEEDED")
            );
        let retry_at = if institution_throttle {
            retry_after.or_else(|| Some(Utc::now() + chrono::Duration::hours(1)))
        } else {
            retry_after
        };

        Err(Self {
            provider: provider.into(),
            endpoint: safe_endpoint(endpoint),
            kind: "provider",
            http_status: Some(status.as_u16()),
            error_type,
            error_code,
            request_id,
            attempts,
            retryable,
            retry_at,
        })
    }

    pub fn code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }

    pub fn should_retry_now(&self, safe_read: bool, attempts: usize, max_attempts: usize) -> bool {
        safe_read
            && attempts < max_attempts
            && self.retryable
            && self.error_code.as_deref() != Some("INSTITUTION_RATE_LIMIT")
            && self
                .retry_at
                .is_none_or(|time| time <= Utc::now() + chrono::Duration::seconds(2))
    }

    pub fn retry_delay(&self, failed_attempts: usize) -> Duration {
        let jitter = retry_delay(failed_attempts);
        let retry_after = self
            .retry_at
            .and_then(|time| (time - Utc::now()).to_std().ok())
            .filter(|delay| *delay <= Duration::from_secs(2))
            .unwrap_or_default();
        jitter.max(retry_after)
    }

    pub fn diagnostic(&self) -> ProviderDiagnostic {
        ProviderDiagnostic {
            provider: bounded(&self.provider, 32),
            endpoint: bounded(&self.endpoint, 96),
            kind: self.kind.into(),
            http_status: self.http_status,
            error_type: self.error_type.as_deref().map(|value| bounded(value, 64)),
            error_code: self.error_code.as_deref().map(|value| bounded(value, 64)),
            request_ref: self.request_id.as_deref().map(request_reference),
            attempts: self.attempts.min(3),
            retryable: self.retryable,
            retry_at: self.retry_at.map(|value| value.to_rfc3339()),
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.error_code.as_deref() {
            Some("INSTITUTION_RATE_LIMIT") => {
                "Plaid temporarily limited this institution; saved data remains available and a later refresh can retry"
            }
            Some("ITEM_LOGIN_REQUIRED" | "ACCESS_NOT_GRANTED") => {
                "The Plaid connection needs attention in Settings"
            }
            Some("TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION") => {
                "Plaid transactions changed during pagination"
            }
            _ if self.kind == "network" => "The provider could not be reached",
            _ if self.kind == "invalid_response" => "The provider returned an invalid response",
            _ if self.http_status.is_some_and(|status| status >= 500) => {
                "The provider is temporarily unavailable"
            }
            _ if self.http_status == Some(429) => "The provider temporarily limited requests",
            _ => "The provider rejected the request",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProviderError {}

pub fn retry_delay(failed_attempts: usize) -> Duration {
    let ceiling = match failed_attempts {
        1 => 500,
        _ => 1_000,
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.subsec_nanos() as u64)
        .unwrap_or_default();
    Duration::from_millis(nanos % (ceiling + 1))
}

fn retry_after(response: &reqwest::Response) -> Option<DateTime<Utc>> {
    let value = response.headers().get(RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = value.parse::<u64>() {
        return (seconds <= 86_400).then(|| {
            Utc::now() + chrono::Duration::seconds(i64::try_from(seconds).unwrap_or(i64::MAX))
        });
    }
    DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn safe_metadata(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        })
        .map(str::to_owned)
}

fn safe_endpoint(path: &str) -> String {
    let path = path.split('?').next().unwrap_or_default();
    if path.starts_with("/accounts/") && !path.starts_with("/accounts/balance/") {
        return redact_path_segment(path, 2, ":id");
    }
    if path.starts_with("/v2/stocks/") {
        return redact_path_segment(path, 3, ":symbol");
    }
    bounded(path, 96)
}

fn redact_path_segment(path: &str, index: usize, replacement: &str) -> String {
    let mut parts = path.split('/').collect::<Vec<_>>();
    if let Some(part) = parts.get_mut(index) {
        *part = replacement;
    }
    bounded(&parts.join("/"), 96)
}

fn request_reference(request_id: &str) -> String {
    let digest = Sha256::digest(request_id.as_bytes());
    format!("{:x}", digest)[..12].to_owned()
}

fn bounded(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_are_bounded_and_identifiers_are_not_persisted() {
        assert!(retry_delay(1) <= Duration::from_millis(500));
        assert!(retry_delay(2) <= Duration::from_millis(1_000));
        assert_eq!(
            safe_endpoint("/accounts/private-id/positions/all?token=secret"),
            "/accounts/:id/positions/all"
        );
        let reference = request_reference("sensitive-request-id");
        assert_eq!(reference.len(), 12);
        assert!(!reference.contains("sensitive"));
    }

    #[tokio::test]
    async fn rejects_oversized_chunked_responses() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n10\r\n0123456789abcdef\r\n0\r\n\r\n")
                .unwrap();
        });
        let response = reqwest::get(format!("http://{address}/large"))
            .await
            .unwrap();
        assert!(response_bytes(response, 8).await.is_err());
        server.join().unwrap();
    }

    #[tokio::test]
    async fn captures_safe_provider_metadata_without_persisting_raw_messages() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            let body = r#"{"error_type":"RATE_LIMIT_EXCEEDED","error_code":"INSTITUTION_RATE_LIMIT","error_message":"account 123 has secret details","request_id":"provider-request-id"}"#;
            write!(
                stream,
                "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let response = reqwest::get(format!("http://{address}/failure"))
            .await
            .unwrap();
        let error = ProviderError::from_response(
            "Plaid",
            "/accounts/balance/get?access_token=secret",
            response,
            1,
        )
        .await
        .unwrap_err();
        server.join().unwrap();

        let diagnostic = error.diagnostic();
        assert_eq!(
            diagnostic.error_code.as_deref(),
            Some("INSTITUTION_RATE_LIMIT")
        );
        assert_eq!(diagnostic.http_status, Some(429));
        assert_eq!(diagnostic.endpoint, "/accounts/balance/get");
        assert_eq!(diagnostic.request_ref.as_deref().map(str::len), Some(12));
        assert!(!serde_json::to_string(&diagnostic)
            .unwrap()
            .contains("secret details"));
        assert!(!error.should_retry_now(true, 1, 3));
    }

    #[test]
    fn mutation_and_institution_throttles_are_never_retried_immediately() {
        let mut error = ProviderError::network("Plaid", "/item/remove", 1);
        assert!(!error.should_retry_now(false, 1, 3));
        assert!(error.should_retry_now(true, 1, 3));
        error.error_code = Some("INSTITUTION_RATE_LIMIT".into());
        assert!(!error.should_retry_now(true, 1, 3));
    }
}
