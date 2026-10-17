//! Exa, the web search research uses to find and re-read the published terms behind a rule.
//! Only a generic query built from public names, or a rule's source URL, leaves the Mac; the
//! pages that come back are data, never instructions.
use super::*;

/// How much of each page research keeps: enough to quote terms, not a whole site.
pub const PAGE_CHARACTERS: usize = 6_000;
/// Pages per search; the review sheet shows each.
const RESULTS: usize = 3;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExaCredentials {
    pub(super) api_key: String,
}

/// A page Exa returned, as research keeps it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchPage")]
pub struct Page {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub published: Option<String>,
}

#[derive(Deserialize)]
struct Results {
    #[serde(default)]
    results: Vec<Result_>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Result_ {
    url: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    published_date: Option<String>,
}

/// The pages in an Exa response, each kept to an https URL and capped text.
pub(crate) fn pages(body: &[u8]) -> Result<Vec<Page>, String> {
    let results: Results =
        serde_json::from_slice(body).map_err(|_| "Exa returned a response Brief can't read")?;
    Ok(results
        .results
        .into_iter()
        .filter(|result| result.url.starts_with("https://"))
        .map(|result| Page {
            url: result.url,
            title: result.title.unwrap_or_default().trim().to_owned(),
            text: result
                .text
                .unwrap_or_default()
                .chars()
                .take(PAGE_CHARACTERS)
                .collect(),
            published: result
                .published_date
                .and_then(|date| date.get(..10).map(str::to_owned)),
        })
        .collect())
}

impl Providers {
    pub(super) fn exa_credentials(&self) -> Result<ExaCredentials, String> {
        self.read_secret(EXA_CREDENTIALS_KEY)?
            .ok_or_else(|| "Enter an Exa API key in Settings first".into())
    }

    async fn exa_post(
        &self,
        credentials: &ExaCredentials,
        path: &str,
        body: Value,
    ) -> Result<Vec<Page>, String> {
        let response = self
            .http
            .post(format!("https://api.exa.ai{path}"))
            .header("x-api-key", &credentials.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|_| "Could not reach Exa")?;
        match response.status() {
            status if status.is_success() => {}
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                return Err("Exa rejected the API key".into())
            }
            reqwest::StatusCode::TOO_MANY_REQUESTS => {
                return Err("Exa is limiting requests; try again later".into())
            }
            status => return Err(format!("Exa returned {status}")),
        }
        let body = http::response_bytes(response, http::MAX_PROVIDER_RESPONSE_BYTES)
            .await
            .map_err(|error| error.to_string())?;
        pages(&body)
    }

    async fn exa_search_with(
        &self,
        credentials: &ExaCredentials,
        query: &str,
        results: usize,
    ) -> Result<Vec<Page>, String> {
        self.exa_post(
            credentials,
            "/search",
            json!({
                "query": query,
                "type": "auto",
                "numResults": results,
                "contents": { "text": { "maxCharacters": PAGE_CHARACTERS } },
            }),
        )
        .await
    }

    /// Searches for pages that state a program's terms.
    pub async fn exa_search(&self, query: &str) -> Result<Vec<Page>, String> {
        let credentials = self.exa_credentials()?;
        self.exa_search_with(&credentials, query, RESULTS).await
    }

    /// Reads pages by URL, as the monthly re-check of rule sources does.
    pub async fn exa_contents(&self, urls: &[String]) -> Result<Vec<Page>, String> {
        let credentials = self.exa_credentials()?;
        self.exa_post(
            &credentials,
            "/contents",
            json!({ "urls": urls, "text": { "maxCharacters": PAGE_CHARACTERS } }),
        )
        .await
    }

    /// Proves a key works with one search for a public page, before saving it.
    pub(super) async fn exa_check(&self, credentials: &ExaCredentials) -> Result<(), String> {
        self.exa_search_with(credentials, "Exa search API documentation", 1)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_keep_https_results_with_capped_text() {
        let long = "a".repeat(PAGE_CHARACTERS + 50);
        let body = json!({
            "requestId": "r",
            "results": [
                { "url": "https://bank.example/terms", "title": " Terms ", "text": long,
                  "publishedDate": "2026-01-02T00:00:00.000Z" },
                { "url": "http://insecure.example/terms", "title": "Plain" },
                { "url": "https://bank.example/other" }
            ]
        });
        let pages = pages(body.to_string().as_bytes()).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].title, "Terms");
        assert_eq!(pages[0].text.chars().count(), PAGE_CHARACTERS);
        assert_eq!(pages[0].published.as_deref(), Some("2026-01-02"));
        assert_eq!(pages[1].text, "");
        assert!(super::pages(b"not json").is_err());
    }
}
