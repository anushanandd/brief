use super::*;

impl Providers {
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
        let _guard = self
            .vault
            .lock()
            .map_err(|_| "The secure credential state is unavailable".to_string())?;
        Ok(())
    }

    pub fn unlock_credentials(&self) -> Result<bool, String> {
        Ok(!self.credential_vault()?.entries.is_empty())
    }

    pub(super) fn credential_vault(&self) -> Result<CredentialVault, String> {
        let _guard = self
            .vault
            .lock()
            .map_err(|_| "The secure credential state is unavailable".to_string())?;
        Ok(match read_protected_vault()? {
            Some(vault) => vault,
            None => migrate_legacy_credentials()?,
        })
    }

    pub fn integration_status(&self) -> Result<IntegrationStatus, String> {
        let vault = self.credential_vault()?;
        Ok(IntegrationStatus {
            plaid: vault.entries.contains_key(PLAID_CREDENTIALS_KEY),
            snaptrade: vault.entries.contains_key(SNAPTRADE_CREDENTIALS_KEY),
            alpaca: vault.entries.contains_key(ALPACA_CREDENTIALS_KEY),
            alpha_vantage: vault.entries.contains_key(ALPHA_VANTAGE_CREDENTIALS_KEY),
        })
    }

    pub(super) fn read_secret<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, String> {
        self.credential_vault()?
            .entries
            .get(key)
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| error.to_string())
    }

    pub(super) fn mutate_vault(
        &self,
        expected: Option<u64>,
        credentials_changed: bool,
        change: impl FnOnce(&mut CredentialVault) -> Result<(), String>,
    ) -> Result<(), String> {
        self.mutate_vault_with(expected, credentials_changed, change, write_protected_vault)
    }

    pub(super) fn mutate_vault_with(
        &self,
        expected: Option<u64>,
        credentials_changed: bool,
        change: impl FnOnce(&mut CredentialVault) -> Result<(), String>,
        persist: impl FnOnce(&CredentialVault) -> Result<(), String>,
    ) -> Result<(), String> {
        let _guard = self
            .vault
            .lock()
            .map_err(|_| "The secure credential state is unavailable")?;
        if expected.is_some_and(|generation| {
            generation != self.credential_generation.load(Ordering::SeqCst)
        }) {
            return Err("Credentials changed; start a new connection attempt".into());
        }
        let mut vault = match read_protected_vault()? {
            Some(vault) => vault,
            None => migrate_legacy_credentials()?,
        };
        persist_vault_change(&mut vault, change, persist)?;
        if credentials_changed {
            self.credential_generation.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    pub(super) fn write_secret<T: Serialize>(&self, key: &str, value: &T) -> Result<(), String> {
        self.mutate_vault(None, true, |vault| {
            vault.entries.insert(
                key.into(),
                serde_json::to_value(value).map_err(|error| error.to_string())?,
            );
            Ok(())
        })
    }

    pub(super) fn write_plaid_credentials(
        &self,
        credentials: &PlaidCredentials,
    ) -> Result<(), String> {
        self.mutate_vault(None, true, |vault| {
            replace_plaid_credentials(vault, credentials)
        })
    }

    pub(super) fn plaid_credentials(&self) -> Result<PlaidCredentials, String> {
        self.read_secret(PLAID_CREDENTIALS_KEY)?
            .ok_or_else(|| "Enter and test Plaid credentials in Settings first".into())
    }

    pub(super) fn snaptrade_credentials(&self) -> Result<SnapTradeCredentials, String> {
        self.read_secret(SNAPTRADE_CREDENTIALS_KEY)?
            .ok_or_else(|| "Enter and test SnapTrade credentials in Settings first".into())
    }

    pub(super) fn alpaca_credentials(&self) -> Result<AlpacaCredentials, String> {
        self.read_secret(ALPACA_CREDENTIALS_KEY)?
            .ok_or_else(|| "Enter and test Alpaca credentials in Settings first".into())
    }

    pub(super) fn alpha_vantage_credentials(&self) -> Result<AlphaVantageCredentials, String> {
        self.read_secret(ALPHA_VANTAGE_CREDENTIALS_KEY)?
            .ok_or_else(|| "Enter and test an Alpha Vantage API key in Settings first".into())
    }

    pub async fn save_credentials(
        &self,
        provider: &str,
        client_id: String,
        secret: Option<String>,
        consumer_key: Option<String>,
    ) -> Result<(), String> {
        let client_id = required(
            client_id,
            if provider == "alpaca" {
                "API key ID"
            } else if provider == "alphavantage" {
                "API key"
            } else {
                "Client ID"
            },
        )?;
        match provider {
            "plaid" => {
                let credentials = PlaidCredentials {
                    client_id,
                    secret: required(secret.unwrap_or_default(), "Plaid secret")?,
                };
                self.plaid_once(
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
                .await
                .map_err(|error| error.to_string())?;
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
            "alpaca" => {
                let credentials = AlpacaCredentials {
                    key_id: client_id,
                    secret_key: required(secret.unwrap_or_default(), "Alpaca secret key")?,
                };
                self.alpaca_request(&credentials, "/v2/stocks/AAPL/snapshot", &[("feed", "iex")])
                    .await?;
                self.write_secret(ALPACA_CREDENTIALS_KEY, &credentials)?;
            }
            "alphavantage" => {
                let credentials = AlphaVantageCredentials { api_key: client_id };
                self.alpha_vantage_earnings(Some("IBM"), &credentials)
                    .await?;
                self.write_secret(ALPHA_VANTAGE_CREDENTIALS_KEY, &credentials)?;
            }
            _ => return Err("Unknown provider".into()),
        }
        Ok(())
    }
}
