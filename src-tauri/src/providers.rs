use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration as StdDuration, Instant},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;

const KEYCHAIN_SERVICE: &str = "com.brief.finance.providers";
const CREDENTIAL_VAULT_KEY: &str = "credential-vault-v2";
const PLAID_CREDENTIALS_KEY: &str = "plaid-credentials";
const PLAID_ITEMS_KEY: &str = "plaid-items";
const PLAID_USER_KEY: &str = "plaid-user";
const SNAPTRADE_CREDENTIALS_KEY: &str = "snaptrade-credentials";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub plaid: bool,
    pub snaptrade: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSession {
    pub provider: String,
    pub session_id: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkStatus {
    pub status: String,
    pub accounts: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSync {
    pub plaid: PlaidData,
    pub snaptrade: SnapTradeData,
    pub net_worth_history: Value,
    pub net_worth_history_estimated: bool,
    pub benchmark_history: Value,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PlaidData {
    pub accounts: Vec<Value>,
    pub transactions: Vec<Value>,
    #[serde(rename = "ignoredAccounts")]
    pub ignored_accounts: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SnapTradeData {
    pub accounts: Vec<Value>,
    pub positions: BTreeMap<String, Vec<Value>>,
    pub activities: BTreeMap<String, Vec<Value>>,
    #[serde(rename = "balanceHistory")]
    pub balance_history: BTreeMap<String, Vec<Value>>,
    #[serde(rename = "ignoredAccounts")]
    pub ignored_accounts: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaidCredentials {
    client_id: String,
    secret: String,
    environment: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapTradeCredentials {
    client_id: String,
    consumer_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaidItem {
    item_id: String,
    access_token: String,
    institution_id: Option<String>,
    institution_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaidUser {
    client_id: String,
    user_id: Option<String>,
    user_token: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct CredentialVault {
    entries: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaidCache {
    items: BTreeMap<String, PlaidItemCache>,
}

impl PlaidCache {
    pub fn transactions(&self) -> impl Iterator<Item = &Value> {
        self.items
            .values()
            .flat_map(|item| item.transactions.iter())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaidItemCache {
    cursor: Option<String>,
    transactions: Vec<Value>,
}

#[derive(Clone, Debug)]
enum PendingLink {
    Plaid {
        link_token: String,
        exchanged_public_tokens: Vec<String>,
    },
    SnapTrade,
}

pub struct Providers {
    http: reqwest::Client,
    links: Mutex<BTreeMap<String, PendingLink>>,
    vault: Mutex<Option<CredentialVault>>,
    credential_edit_authorized_until: Mutex<Option<Instant>>,
}

impl Providers {
    pub fn new() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .user_agent("Brief/0.1")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            http,
            links: Mutex::new(BTreeMap::new()),
            vault: Mutex::new(None),
            credential_edit_authorized_until: Mutex::new(None),
        })
    }

    pub fn authorize_credential_edit(&self) -> Result<(), String> {
        *self
            .credential_edit_authorized_until
            .lock()
            .map_err(|_| "Credential authorization state is unavailable".to_string())? =
            Some(Instant::now() + StdDuration::from_secs(120));
        Ok(())
    }

    pub fn take_credential_edit_authorization(&self) -> Result<bool, String> {
        let authorized_until = self
            .credential_edit_authorized_until
            .lock()
            .map_err(|_| "Credential authorization state is unavailable".to_string())?
            .take();
        Ok(authorized_until.is_some_and(|deadline| deadline > Instant::now()))
    }

    pub fn lock_credentials(&self) -> Result<(), String> {
        *self
            .vault
            .lock()
            .map_err(|_| "The secure credential cache is unavailable".to_string())? = None;
        Ok(())
    }

    /// Unlock all provider secrets in one Keychain operation. On macOS the vault itself requires
    /// user presence, so reading it supplies the system authentication prompt.
    pub fn unlock_credentials(&self) -> Result<bool, String> {
        Ok(!self.credential_vault()?.entries.is_empty())
    }

    fn credential_vault(&self) -> Result<CredentialVault, String> {
        let mut cached = self
            .vault
            .lock()
            .map_err(|_| "The secure credential cache is unavailable".to_string())?;
        if let Some(vault) = cached.as_ref() {
            return Ok(vault.clone());
        }

        let vault = match read_protected_vault()? {
            Some(vault) => vault,
            None => migrate_legacy_credentials()?,
        };
        *cached = Some(vault.clone());
        Ok(vault)
    }

    fn read_secret<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, String> {
        self.credential_vault()?
            .entries
            .get(key)
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| error.to_string())
    }

    fn write_secret<T: Serialize>(&self, key: &str, value: &T) -> Result<(), String> {
        let mut vault = self.credential_vault()?;
        vault.entries.insert(
            key.into(),
            serde_json::to_value(value).map_err(|error| error.to_string())?,
        );
        write_protected_vault(&vault)?;
        *self
            .vault
            .lock()
            .map_err(|_| "The secure credential cache is unavailable".to_string())? = Some(vault);
        Ok(())
    }

    fn write_plaid_credentials(&self, credentials: &PlaidCredentials) -> Result<(), String> {
        let mut vault = self.credential_vault()?;
        replace_plaid_credentials(&mut vault, credentials)?;
        write_protected_vault(&vault)?;
        *self
            .vault
            .lock()
            .map_err(|_| "The secure credential cache is unavailable".to_string())? = Some(vault);
        Ok(())
    }

    fn plaid_credentials(&self) -> Result<PlaidCredentials, String> {
        self.read_secret(PLAID_CREDENTIALS_KEY)?
            .ok_or_else(|| "Enter and test Plaid credentials in Settings first".into())
    }

    fn snaptrade_credentials(&self) -> Result<SnapTradeCredentials, String> {
        self.read_secret(SNAPTRADE_CREDENTIALS_KEY)?
            .ok_or_else(|| "Enter and test SnapTrade credentials in Settings first".into())
    }

    pub async fn save_credentials(
        &self,
        provider: &str,
        client_id: String,
        secret: Option<String>,
        consumer_key: Option<String>,
    ) -> Result<(), String> {
        let client_id = required(client_id, "Client ID")?;
        match provider {
            "plaid" => {
                let credentials = PlaidCredentials {
                    client_id,
                    secret: required(secret.unwrap_or_default(), "Plaid secret")?,
                    environment: "production".into(),
                };
                self.plaid_request(
                    &credentials,
                    "/link/token/create",
                    json!({
                        "client_name": "Brief",
                        "country_codes": ["US"],
                        "language": "en",
                        "products": ["transactions"],
                        "user": { "client_user_id": "brief-owner" }
                    }),
                )
                .await?;
                self.write_plaid_credentials(&credentials)?;
            }
            "snaptrade" => {
                let credentials = SnapTradeCredentials {
                    client_id,
                    consumer_key: required(
                        consumer_key.unwrap_or_default(),
                        "SnapTrade consumer key",
                    )?,
                };
                self.snaptrade_request(&credentials, Method::GET, "/accounts", vec![], None)
                    .await?;
                self.write_secret(SNAPTRADE_CREDENTIALS_KEY, &credentials)?;
            }
            _ => return Err("Unknown provider".into()),
        }
        Ok(())
    }

    pub async fn begin_link(&self, provider: &str) -> Result<LinkSession, String> {
        let session_id = Uuid::new_v4().to_string();
        let (url, pending) = match provider {
            "plaid" => {
                let credentials = self.plaid_credentials()?;
                let user = self.ensure_plaid_user(&credentials).await?;
                let (user_field, user_identifier) = plaid_user_identifier(&user)?;
                let mut request = json!({
                    "account_filters": {
                        "credit": { "account_subtypes": ["credit card"] },
                        "depository": { "account_subtypes": ["checking", "savings"] }
                    },
                    "client_name": "Brief",
                    "country_codes": ["US"],
                    "enable_multi_item_link": true,
                    "hosted_link": { "url_lifetime_seconds": 1800 },
                    "language": "en",
                    "products": ["transactions"],
                    "transactions": { "days_requested": 730 }
                });
                request[user_field] = Value::String(user_identifier.into());
                let response = self
                    .plaid_request(&credentials, "/link/token/create", request)
                    .await?;
                let link_token = string_field(&response, "link_token")?;
                let url = string_field(&response, "hosted_link_url")?;
                (
                    url,
                    PendingLink::Plaid {
                        link_token,
                        exchanged_public_tokens: Vec::new(),
                    },
                )
            }
            "snaptrade" => {
                let credentials = self.snaptrade_credentials()?;
                let response = self
                    .snaptrade_request(
                        &credentials,
                        Method::POST,
                        "/snapTrade/login",
                        vec![],
                        Some(json!({
                            "connectionType": "read",
                            "showCloseButton": true
                        })),
                    )
                    .await?;
                (
                    string_field(&response, "redirectURI")?,
                    PendingLink::SnapTrade,
                )
            }
            _ => return Err("Unknown provider".into()),
        };
        self.links
            .lock()
            .map_err(|_| "Provider link state is unavailable".to_string())?
            .insert(session_id.clone(), pending);
        Ok(LinkSession {
            provider: provider.into(),
            session_id,
            url,
        })
    }

    pub async fn poll_link(&self, provider: &str, session_id: &str) -> Result<LinkStatus, String> {
        let pending = self
            .links
            .lock()
            .map_err(|_| "Provider link state is unavailable".to_string())?
            .get(session_id)
            .cloned()
            .ok_or_else(|| "Unknown or expired provider link session".to_string())?;

        match (provider, pending) {
            (
                "plaid",
                PendingLink::Plaid {
                    link_token,
                    exchanged_public_tokens,
                },
            ) => {
                let credentials = self.plaid_credentials()?;
                let response = self
                    .plaid_request(
                        &credentials,
                        "/link/token/get",
                        json!({ "link_token": link_token }),
                    )
                    .await?;
                let public_tokens = plaid_public_tokens(&response);
                if public_tokens.is_empty() {
                    return Ok(pending_status());
                }

                let mut items = self
                    .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
                    .unwrap_or_default();
                let mut exchanged = exchanged_public_tokens;
                for public_token in public_tokens {
                    if exchanged.contains(&public_token) {
                        continue;
                    }
                    let exchange = self
                        .plaid_request(
                            &credentials,
                            "/item/public_token/exchange",
                            json!({ "public_token": public_token }),
                        )
                        .await?;
                    let item_id = string_field(&exchange, "item_id")?;
                    let access_token = string_field(&exchange, "access_token")?;
                    if !items.iter().any(|item| item.item_id == item_id) {
                        items.push(PlaidItem {
                            item_id,
                            access_token: access_token.clone(),
                            institution_id: None,
                            institution_name: None,
                        });
                        // Public tokens are one-time credentials. Persist each access token before
                        // any optional metadata request or the next exchange can fail.
                        self.write_secret(PLAID_ITEMS_KEY, &items)?;

                        let item = self
                            .plaid_request(
                                &credentials,
                                "/item/get",
                                json!({ "access_token": &access_token }),
                            )
                            .await
                            .ok();
                        let institution_id = item
                            .as_ref()
                            .and_then(|value| value.pointer("/item/institution_id"))
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        let institution_name = if let Some(id) = institution_id.as_deref() {
                            self.plaid_request(
                                &credentials,
                                "/institutions/get_by_id",
                                json!({ "institution_id": id, "country_codes": ["US"] }),
                            )
                            .await
                            .ok()
                            .as_ref()
                            .and_then(|value| value.pointer("/institution/name"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                        } else {
                            None
                        };
                        if institution_id.is_some() || institution_name.is_some() {
                            if let Some(stored) = items.last_mut() {
                                stored.institution_id = institution_id;
                                stored.institution_name = institution_name;
                            }
                            self.write_secret(PLAID_ITEMS_KEY, &items)?;
                        }
                    }
                    exchanged.push(public_token);
                    if let Some(PendingLink::Plaid {
                        exchanged_public_tokens,
                        ..
                    }) = self
                        .links
                        .lock()
                        .map_err(|_| "Provider link state is unavailable".to_string())?
                        .get_mut(session_id)
                    {
                        *exchanged_public_tokens = exchanged.clone();
                    }
                }
                self.finish_link(session_id)?;
                Ok(LinkStatus {
                    status: "connected".into(),
                    accounts: Vec::new(),
                })
            }
            ("snaptrade", PendingLink::SnapTrade) => {
                let data = self.sync_snaptrade().await?;
                if data.accounts.is_empty() {
                    Ok(pending_status())
                } else {
                    self.finish_link(session_id)?;
                    Ok(LinkStatus {
                        status: "connected".into(),
                        accounts: data
                            .accounts
                            .iter()
                            .filter_map(|account| account.get("name").and_then(Value::as_str))
                            .map(str::to_owned)
                            .collect(),
                    })
                }
            }
            _ => Err("Provider does not match this link session".into()),
        }
    }

    fn finish_link(&self, session_id: &str) -> Result<(), String> {
        self.links
            .lock()
            .map_err(|_| "Provider link state is unavailable".to_string())?
            .remove(session_id);
        Ok(())
    }

    pub async fn sync_plaid(&self, cache: &mut PlaidCache) -> Result<PlaidData, String> {
        let Some(credentials) = self.read_secret::<PlaidCredentials>(PLAID_CREDENTIALS_KEY)? else {
            return Ok(PlaidData::default());
        };
        let items = self
            .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
            .unwrap_or_default();
        let mut data = PlaidData::default();
        for item in items {
            let item_cache = cache.items.entry(item.item_id.clone()).or_default();
            loop {
                let mut request = json!({
                    "access_token": item.access_token,
                    "count": 500
                });
                if let Some(cursor) = item_cache.cursor.as_deref() {
                    request["cursor"] = Value::String(cursor.into());
                }
                let sync = self
                    .plaid_request(&credentials, "/transactions/sync", request)
                    .await?;
                let mut transactions: BTreeMap<String, Value> = item_cache
                    .transactions
                    .drain(..)
                    .filter_map(|transaction| {
                        let id = transaction
                            .get("transaction_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned)?;
                        Some((id, transaction))
                    })
                    .collect();
                for transaction in arrays(&sync, &["added", "modified"]) {
                    if let Some(id) = transaction.get("transaction_id").and_then(Value::as_str) {
                        transactions.insert(id.to_owned(), transaction);
                    }
                }
                for removed in array(&sync, "removed") {
                    if let Some(id) = removed.get("transaction_id").and_then(Value::as_str) {
                        transactions.remove(id);
                    }
                }
                item_cache.transactions = transactions.into_values().collect();
                item_cache.cursor = sync
                    .get("next_cursor")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if !sync
                    .get("has_more")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    break;
                }
            }

            let balances = self
                .plaid_request(
                    &credentials,
                    "/accounts/balance/get",
                    json!({ "access_token": item.access_token }),
                )
                .await?;
            let mut allowed_ids = Vec::new();
            for mut account in array(&balances, "accounts") {
                let institution = item
                    .institution_name
                    .as_deref()
                    .unwrap_or("Unknown institution");
                let name = account
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Account");
                if allowed_plaid_account(&account) {
                    if let Some(object) = account.as_object_mut() {
                        object.insert("institution_name".into(), Value::String(institution.into()));
                    }
                    if let Some(id) = account.get("account_id").and_then(Value::as_str) {
                        allowed_ids.push(id.to_owned());
                    }
                    data.accounts.push(account);
                } else {
                    data.ignored_accounts
                        .push(format!("{institution} · {name}"));
                }
            }
            data.transactions.extend(
                item_cache
                    .transactions
                    .iter()
                    .filter(|transaction| {
                        transaction
                            .get("account_id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| allowed_ids.iter().any(|allowed| allowed == id))
                    })
                    .cloned(),
            );
        }
        Ok(data)
    }

    pub async fn sync_snaptrade(&self) -> Result<SnapTradeData, String> {
        let Some(credentials) =
            self.read_secret::<SnapTradeCredentials>(SNAPTRADE_CREDENTIALS_KEY)?
        else {
            return Ok(SnapTradeData::default());
        };
        let all_accounts = self
            .snaptrade_request(&credentials, Method::GET, "/accounts", vec![], None)
            .await?
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut data = SnapTradeData::default();
        for account in all_accounts {
            let id = account
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let institution = account
                .get("institution_name")
                .and_then(Value::as_str)
                .unwrap_or("Unknown institution");
            let name = account
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let raw_type = account
                .get("raw_type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            // `/accounts` only returns brokerage accounts. `account_category` is optional and is
            // commonly null for valid accounts, including E*Trade Individual and Roth IRA
            // accounts, so it cannot be used as an inclusion filter.
            if supported_snaptrade_account(&account) {
                data.accounts.push(account);
                let positions_path = format!("/accounts/{id}/positions/all");
                let history_path = format!("/accounts/{id}/balanceHistory");
                let activities_path = format!("/accounts/{id}/activities");
                let since = (Utc::now() - Duration::days(730))
                    .format("%Y-%m-%d")
                    .to_string();
                let (positions, balance_history, activities) = tokio::join!(
                    self.snaptrade_request(
                        &credentials,
                        Method::GET,
                        &positions_path,
                        vec![],
                        None,
                    ),
                    tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        self.snaptrade_request(
                            &credentials,
                            Method::GET,
                            &history_path,
                            vec![],
                            None,
                        ),
                    ),
                    self.snaptrade_activities(&credentials, &activities_path, &since),
                );
                let positions = positions?;
                data.positions.insert(
                    id.clone(),
                    positions
                        .get("results")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                );
                let balance_history = balance_history
                    .ok()
                    .and_then(Result::ok)
                    .and_then(|response| response.get("history").and_then(Value::as_array).cloned())
                    .unwrap_or_default();
                data.balance_history.insert(id.clone(), balance_history);
                data.activities.insert(id, activities?);
            } else {
                data.ignored_accounts.push(format!(
                    "{institution} · {}",
                    if name.is_empty() { raw_type } else { name }
                ));
            }
        }
        Ok(data)
    }

    async fn snaptrade_activities(
        &self,
        credentials: &SnapTradeCredentials,
        path: &str,
        since: &str,
    ) -> Result<Vec<Value>, String> {
        const PAGE_SIZE: usize = 1000;

        let mut activities = Vec::new();
        loop {
            let response = self
                .snaptrade_request(
                    credentials,
                    Method::GET,
                    path,
                    vec![
                        ("startDate".into(), since.into()),
                        ("limit".into(), PAGE_SIZE.to_string()),
                        ("offset".into(), activities.len().to_string()),
                    ],
                    None,
                )
                .await?;
            let page = response
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let page_len = page.len();
            activities.extend(page);
            let total = response
                .pointer("/pagination/total")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok());

            if page_len == 0 || total.is_some_and(|value| activities.len() >= value) {
                break;
            }
            // Older API responses may omit pagination metadata. A short page is still a reliable
            // end condition, while a full page warrants one more request.
            if total.is_none() && page_len < PAGE_SIZE {
                break;
            }
        }
        Ok(activities)
    }

    pub async fn sync_sp500(&self) -> Result<Value, String> {
        let fred = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            self.sync_sp500_from_fred(),
        )
        .await;
        if let Ok(Ok(history)) = fred {
            if history.as_array().is_some_and(|points| !points.is_empty()) {
                return Ok(history);
            }
        }

        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.sync_sp500_from_yahoo(),
        )
        .await
        .map_err(|_| "S&P 500 history providers timed out".to_string())?
    }

    async fn sync_sp500_from_fred(&self) -> Result<Value, String> {
        let since = (Utc::now() - Duration::days(370))
            .format("%Y-%m-%d")
            .to_string();
        let response = self
            .http
            .get("https://fred.stlouisfed.org/graph/fredgraph.csv")
            .query(&[("id", "SP500"), ("cosd", since.as_str())])
            .send()
            .await
            .map_err(|error| format!("Could not reach FRED: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("FRED returned {}", response.status()));
        }
        let csv = response
            .text()
            .await
            .map_err(|error| format!("FRED returned an invalid response: {error}"))?;
        Ok(parse_sp500_csv(&csv))
    }

    async fn sync_sp500_from_yahoo(&self) -> Result<Value, String> {
        let response = self
            .http
            .get("https://query1.finance.yahoo.com/v8/finance/chart/%5EGSPC")
            .query(&[("range", "1y"), ("interval", "1d"), ("events", "history")])
            .send()
            .await
            .map_err(|error| format!("Could not reach the S&P 500 fallback: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("S&P 500 fallback returned {}", response.status()));
        }
        let payload: Value = response
            .json()
            .await
            .map_err(|error| format!("S&P 500 fallback returned invalid data: {error}"))?;
        let history = parse_yahoo_sp500(&payload);
        if history.as_array().is_some_and(|points| !points.is_empty()) {
            Ok(history)
        } else {
            Err("S&P 500 fallback returned no daily closes".into())
        }
    }

    async fn ensure_plaid_user(&self, credentials: &PlaidCredentials) -> Result<PlaidUser, String> {
        if let Some(user) = self.read_secret::<PlaidUser>(PLAID_USER_KEY)? {
            if user.client_id == credentials.client_id && plaid_user_identifier(&user).is_ok() {
                return Ok(user);
            }
        }

        let response = self
            .plaid_request(
                credentials,
                "/user/create",
                json!({ "client_user_id": "brief-local-owner" }),
            )
            .await?;
        let user = PlaidUser {
            client_id: credentials.client_id.clone(),
            user_id: response
                .get("user_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            user_token: response
                .get("user_token")
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
        plaid_user_identifier(&user)?;
        self.write_secret(PLAID_USER_KEY, &user)?;
        Ok(user)
    }

    async fn plaid_request(
        &self,
        credentials: &PlaidCredentials,
        path: &str,
        body: Value,
    ) -> Result<Value, String> {
        let host = match credentials.environment.as_str() {
            "sandbox" => "https://sandbox.plaid.com",
            _ => "https://production.plaid.com",
        };
        let response = self
            .http
            .post(format!("{host}{path}"))
            .header("PLAID-CLIENT-ID", &credentials.client_id)
            .header("PLAID-SECRET", &credentials.secret)
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("Could not reach Plaid: {error}"))?;
        provider_response("Plaid", response).await
    }

    async fn snaptrade_request(
        &self,
        credentials: &SnapTradeCredentials,
        method: Method,
        path: &str,
        parameters: Vec<(String, String)>,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut url = Url::parse(&format!("https://api.snaptrade.com{path}"))
            .map_err(|error| error.to_string())?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("clientId", &credentials.client_id);
            query.append_pair("timestamp", &Utc::now().timestamp().to_string());
            for (key, value) in parameters {
                query.append_pair(&key, &value);
            }
        }
        let signature_payload = json!({
            "content": body.clone().filter(|value| !value.as_object().is_some_and(|v| v.is_empty())).unwrap_or(Value::Null),
            "path": url.path(),
            "query": url.query().unwrap_or_default()
        });
        let signature_content =
            serde_json::to_string(&signature_payload).map_err(|error| error.to_string())?;
        let mut mac = Hmac::<Sha256>::new_from_slice(credentials.consumer_key.as_bytes())
            .map_err(|error| error.to_string())?;
        mac.update(signature_content.as_bytes());
        let signature = BASE64.encode(mac.finalize().into_bytes());
        let mut request = self
            .http
            .request(method, url)
            .header("Signature", signature)
            .header("Accept", "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("Could not reach SnapTrade: {error}"))?;
        provider_response("SnapTrade", response).await
    }
}

fn replace_plaid_credentials(
    vault: &mut CredentialVault,
    credentials: &PlaidCredentials,
) -> Result<(), String> {
    let client_changed = vault
        .entries
        .get(PLAID_CREDENTIALS_KEY)
        .cloned()
        .map(serde_json::from_value::<PlaidCredentials>)
        .transpose()
        .map_err(|error| error.to_string())?
        .is_some_and(|previous| previous.client_id != credentials.client_id);

    if client_changed {
        // Plaid Item access tokens and Multi-Item Link users are scoped to the client. Keeping
        // them after a client switch makes every subsequent sync fail with the new secret.
        vault.entries.remove(PLAID_ITEMS_KEY);
        vault.entries.remove(PLAID_USER_KEY);
    }
    vault.entries.insert(
        PLAID_CREDENTIALS_KEY.into(),
        serde_json::to_value(credentials).map_err(|error| error.to_string())?,
    );
    Ok(())
}

fn parse_sp500_csv(csv: &str) -> Value {
    Value::Array(
        csv.lines()
            .skip(1)
            .filter_map(|line| {
                let (date, raw_value) = line.split_once(',')?;
                let value = raw_value.trim().parse::<f64>().ok()?;
                Some(json!({ "date": date, "value": value }))
            })
            .collect(),
    )
}

fn parse_yahoo_sp500(payload: &Value) -> Value {
    let timestamps = payload
        .pointer("/chart/result/0/timestamp")
        .and_then(Value::as_array);
    let closes = payload
        .pointer("/chart/result/0/indicators/quote/0/close")
        .and_then(Value::as_array);
    let points = timestamps
        .zip(closes)
        .into_iter()
        .flat_map(|(timestamps, closes)| timestamps.iter().zip(closes))
        .filter_map(|(timestamp, close)| {
            let date = chrono::DateTime::from_timestamp(timestamp.as_i64()?, 0)?
                .format("%Y-%m-%d")
                .to_string();
            Some(json!({ "date": date, "value": close.as_f64()? }))
        })
        .collect();
    Value::Array(points)
}

async fn provider_response(provider: &str, response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("{provider} returned an invalid response: {error}"))?;
    if status.is_success() {
        Ok(payload)
    } else {
        let message = payload
            .get("error_message")
            .or_else(|| payload.get("detail"))
            .or_else(|| payload.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("Provider request failed");
        Err(format!("{provider}: {message}"))
    }
}

fn keychain_entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|error| error.to_string())
}

fn read_keychain<T: DeserializeOwned>(user: &str) -> Result<Option<T>, String> {
    match keychain_entry(user)?.get_password() {
        Ok(value) => serde_json::from_str(&value)
            .map(Some)
            .map_err(|error| error.to_string()),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Could not read secure provider settings: {error}")),
    }
}

#[cfg(not(target_os = "macos"))]
fn write_keychain<T: Serialize>(user: &str, value: &T) -> Result<(), String> {
    let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
    keychain_entry(user)?
        .set_password(&encoded)
        .map_err(|error| format!("Could not save provider settings securely: {error}"))
}

fn migrate_legacy_credentials() -> Result<CredentialVault, String> {
    let mut vault = CredentialVault::default();
    for key in [
        PLAID_CREDENTIALS_KEY,
        PLAID_ITEMS_KEY,
        PLAID_USER_KEY,
        SNAPTRADE_CREDENTIALS_KEY,
    ] {
        if let Some(value) = read_keychain::<Value>(key)? {
            vault.entries.insert(key.into(), value);
        }
    }
    if !vault.entries.is_empty() {
        write_protected_vault(&vault)?;
    }
    Ok(vault)
}

#[cfg(target_os = "macos")]
fn read_protected_vault() -> Result<Option<CredentialVault>, String> {
    use security_framework::passwords::{generic_password, PasswordOptions};
    use security_framework_sys::base::errSecItemNotFound;

    let options = PasswordOptions::new_generic_password(KEYCHAIN_SERVICE, CREDENTIAL_VAULT_KEY);
    match generic_password(options) {
        Ok(encoded) => serde_json::from_slice(&encoded)
            .map(Some)
            .map_err(|error| format!("Could not decode secure provider settings: {error}")),
        Err(error) if error.code() == errSecItemNotFound => Ok(None),
        Err(error) => Err(format!(
            "Could not unlock provider settings with system authentication: {error}"
        )),
    }
}

#[cfg(target_os = "macos")]
fn write_protected_vault(vault: &CredentialVault) -> Result<(), String> {
    use security_framework::passwords::{
        set_generic_password_options, AccessControlOptions, PasswordOptions,
    };

    let encoded = serde_json::to_vec(vault).map_err(|error| error.to_string())?;
    let mut options = PasswordOptions::new_generic_password(KEYCHAIN_SERVICE, CREDENTIAL_VAULT_KEY);
    // User presence prefers Touch ID where available and falls back to the Mac login password,
    // matching the authentication flow on Macs without enrolled biometrics.
    options.set_access_control_options(AccessControlOptions::USER_PRESENCE);
    options.set_label("Brief provider credentials");
    options.set_description("Provider credentials protected by system authentication");
    set_generic_password_options(&encoded, options)
        .map_err(|error| format!("Could not protect provider settings with Touch ID: {error}"))
}

#[cfg(not(target_os = "macos"))]
fn read_protected_vault() -> Result<Option<CredentialVault>, String> {
    read_keychain(CREDENTIAL_VAULT_KEY)
}

#[cfg(not(target_os = "macos"))]
fn write_protected_vault(vault: &CredentialVault) -> Result<(), String> {
    write_keychain(CREDENTIAL_VAULT_KEY, vault)
}

fn required(value: String, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value.into())
    }
}

fn string_field(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("Provider response did not include {field}"))
}

fn plaid_user_identifier(user: &PlaidUser) -> Result<(&'static str, &str), String> {
    if let Some(user_token) = user.user_token.as_deref() {
        Ok(("user_token", user_token))
    } else if let Some(user_id) = user.user_id.as_deref() {
        Ok(("user_id", user_id))
    } else {
        Err("Plaid did not return a user ID for Multi-Item Link".into())
    }
}

fn array(value: &Value, field: &str) -> Vec<Value> {
    value
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn arrays(value: &Value, fields: &[&str]) -> Vec<Value> {
    fields
        .iter()
        .flat_map(|field| array(value, field))
        .collect()
}

fn plaid_public_tokens(value: &Value) -> Vec<String> {
    let mut tokens = Vec::new();
    for session in array(value, "link_sessions") {
        if let Some(token) = session
            .pointer("/on_success/public_token")
            .and_then(Value::as_str)
        {
            tokens.push(token.into());
        }
        if let Some(results) = session
            .pointer("/results/item_add_results")
            .and_then(Value::as_array)
        {
            tokens.extend(
                results
                    .iter()
                    .filter_map(|item| item.get("public_token").and_then(Value::as_str))
                    .map(str::to_owned),
            );
        }
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

fn pending_status() -> LinkStatus {
    LinkStatus {
        status: "pending".into(),
        accounts: Vec::new(),
    }
}

fn allowed_plaid_account(account: &Value) -> bool {
    matches!(
        (
            account.get("type").and_then(Value::as_str),
            account.get("subtype").and_then(Value::as_str),
        ),
        (Some("depository"), Some("checking" | "savings")) | (Some("credit"), Some("credit card"))
    )
}

fn supported_snaptrade_account(account: &Value) -> bool {
    account
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaid_imports_supported_cash_and_card_accounts() {
        assert!(allowed_plaid_account(
            &json!({ "type": "depository", "subtype": "checking" })
        ));
        assert!(allowed_plaid_account(
            &json!({ "type": "credit", "subtype": "credit card" })
        ));
        assert!(!allowed_plaid_account(
            &json!({ "type": "investment", "subtype": "brokerage" })
        ));
    }

    #[test]
    fn snaptrade_accepts_accounts_without_an_account_category() {
        let account = json!({
            "id": "account-id",
            "name": "E*Trade Roth IRA",
            "raw_type": "ROTHIRA",
            "account_category": null
        });

        assert!(supported_snaptrade_account(&account));
    }

    #[test]
    fn plaid_uses_new_user_ids_and_supports_legacy_user_tokens() {
        let new_user = PlaidUser {
            client_id: "client".into(),
            user_id: Some("usr_123".into()),
            user_token: None,
        };
        assert_eq!(
            plaid_user_identifier(&new_user).unwrap(),
            ("user_id", "usr_123")
        );

        let legacy_user = PlaidUser {
            client_id: "client".into(),
            user_id: Some("legacy-id".into()),
            user_token: Some("user-production-123".into()),
        };
        assert_eq!(
            plaid_user_identifier(&legacy_user).unwrap(),
            ("user_token", "user-production-123")
        );
    }

    #[test]
    fn changing_plaid_clients_discards_client_scoped_tokens() {
        let previous = PlaidCredentials {
            client_id: "old-client".into(),
            secret: "old-secret".into(),
            environment: "production".into(),
        };
        let replacement = PlaidCredentials {
            client_id: "new-client".into(),
            secret: "new-secret".into(),
            environment: "production".into(),
        };
        let mut vault = CredentialVault::default();
        vault.entries.insert(
            PLAID_CREDENTIALS_KEY.into(),
            serde_json::to_value(previous).unwrap(),
        );
        vault
            .entries
            .insert(PLAID_ITEMS_KEY.into(), json!([{"accessToken": "token"}]));
        vault
            .entries
            .insert(PLAID_USER_KEY.into(), json!({"userId": "user"}));

        replace_plaid_credentials(&mut vault, &replacement).unwrap();

        assert!(!vault.entries.contains_key(PLAID_ITEMS_KEY));
        assert!(!vault.entries.contains_key(PLAID_USER_KEY));
        assert_eq!(
            vault.entries[PLAID_CREDENTIALS_KEY]["clientId"],
            "new-client"
        );
    }

    #[test]
    fn extracts_multi_item_hosted_link_tokens() {
        let tokens = plaid_public_tokens(&json!({
            "link_sessions": [{
                "results": { "item_add_results": [
                    { "public_token": "public-b" },
                    { "public_token": "public-a" }
                ]}
            }]
        }));
        assert_eq!(tokens, vec!["public-a", "public-b"]);
    }

    #[test]
    fn parses_available_sp500_closes_and_skips_missing_days() {
        assert_eq!(
            parse_sp500_csv("observation_date,SP500\n2026-08-28,7780.25\n2026-08-29,.\n"),
            json!([{ "date": "2026-08-28", "value": 7780.25 }])
        );
    }

    #[test]
    fn parses_available_yahoo_sp500_closes_and_skips_nulls() {
        assert_eq!(
            parse_yahoo_sp500(&json!({
                "chart": { "result": [{
                    "timestamp": [1787875200, 1787961600],
                    "indicators": { "quote": [{ "close": [7780.25, null] }] }
                }]}
            })),
            json!([{ "date": "2026-08-28", "value": 7780.25 }])
        );
    }
}
