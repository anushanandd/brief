use super::*;

/// A service whose credentials Brief keeps in the Keychain vault.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Integration {
    Plaid,
    Snaptrade,
    Alpaca,
    Alphavantage,
    Sec,
}

impl Integration {
    pub const ALL: [Self; 5] = [
        Self::Plaid,
        Self::Snaptrade,
        Self::Alpaca,
        Self::Alphavantage,
        Self::Sec,
    ];

    /// The name diagnostics and the renderer use for it.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|integration| integration.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Plaid => "plaid",
            Self::Snaptrade => "snaptrade",
            Self::Alpaca => "alpaca",
            Self::Alphavantage => "alphavantage",
            Self::Sec => "sec",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Plaid => "Plaid",
            Self::Snaptrade => "SnapTrade",
            Self::Alpaca => "Alpaca",
            Self::Alphavantage => "Alpha Vantage",
            Self::Sec => "SEC EDGAR",
        }
    }

    /// The vault entry whose presence means it is connected.
    fn key(self) -> &'static str {
        match self {
            Self::Plaid => PLAID_CREDENTIALS_KEY,
            Self::Snaptrade => SNAPTRADE_CREDENTIALS_KEY,
            Self::Alpaca => ALPACA_CREDENTIALS_KEY,
            Self::Alphavantage => ALPHA_VANTAGE_CREDENTIALS_KEY,
            Self::Sec => SEC_CONTACT_KEY,
        }
    }

    /// Every vault entry removing it clears; Plaid also keeps its Items and user.
    fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Plaid => &[PLAID_CREDENTIALS_KEY, PLAID_ITEMS_KEY, PLAID_USER_KEY],
            Self::Snaptrade => &[SNAPTRADE_CREDENTIALS_KEY],
            Self::Alpaca => &[ALPACA_CREDENTIALS_KEY],
            Self::Alphavantage => &[ALPHA_VANTAGE_CREDENTIALS_KEY],
            Self::Sec => &[SEC_CONTACT_KEY],
        }
    }

    /// What the first credential field holds.
    fn identifier(self) -> &'static str {
        match self {
            Self::Alpaca => "API key ID",
            Self::Alphavantage => "API key",
            Self::Sec => "Contact email",
            Self::Plaid | Self::Snaptrade => "Client ID",
        }
    }
}

impl IntegrationStatus {
    pub fn connected(&self, integration: Integration) -> bool {
        match integration {
            Integration::Plaid => self.plaid,
            Integration::Snaptrade => self.snaptrade,
            Integration::Alpaca => self.alpaca,
            Integration::Alphavantage => self.alpha_vantage,
            Integration::Sec => self.sec,
        }
    }
}

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
        let connected = |integration: Integration| vault.entries.contains_key(integration.key());
        Ok(IntegrationStatus {
            plaid: connected(Integration::Plaid),
            snaptrade: connected(Integration::Snaptrade),
            alpaca: connected(Integration::Alpaca),
            alpha_vantage: connected(Integration::Alphavantage),
            sec: connected(Integration::Sec),
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

    pub fn remove_credentials(&self, integration: Integration) -> Result<(), String> {
        self.mutate_vault(None, true, |vault| {
            for key in integration.keys() {
                vault.entries.remove(*key);
            }
            Ok(())
        })
    }

    pub async fn save_credentials(
        &self,
        integration: Integration,
        client_id: String,
        secret: Option<String>,
        consumer_key: Option<String>,
    ) -> Result<(), String> {
        let client_id = required(client_id, integration.identifier())?;
        match integration {
            Integration::Plaid => {
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
            Integration::Snaptrade => {
                let credentials = SnapTradeCredentials {
                    client_id,
                    consumer_key: required(
                        consumer_key.unwrap_or_default(),
                        "SnapTrade consumer key",
                    )?,
                };
                self.snaptrade_get(&credentials, "/accounts").await?;
                self.write_secret(SNAPTRADE_CREDENTIALS_KEY, &credentials)?;
            }
            Integration::Alpaca => {
                let credentials = AlpacaCredentials {
                    key_id: client_id,
                    secret_key: required(secret.unwrap_or_default(), "Alpaca secret key")?,
                };
                self.alpaca_request(&credentials, "/v2/stocks/AAPL/snapshot", &[("feed", "iex")])
                    .await?;
                self.write_secret(ALPACA_CREDENTIALS_KEY, &credentials)?;
            }
            Integration::Alphavantage => {
                let credentials = AlphaVantageCredentials { api_key: client_id };
                self.alpha_vantage_earnings(Some("IBM"), &credentials)
                    .await?;
                self.write_secret(ALPHA_VANTAGE_CREDENTIALS_KEY, &credentials)?;
            }
            Integration::Sec => {
                let contact = SecContact {
                    email: sec::sec_contact_email(&client_id)?,
                };
                // Loading the ticker map proves EDGAR accepts this contact, and caches it.
                self.sec_ticker_map(&contact, true).await?;
                self.write_secret(SEC_CONTACT_KEY, &contact)?;
            }
        }
        Ok(())
    }
}

pub(super) fn keychain_entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|error| error.to_string())
}

pub(super) fn read_keychain<T: DeserializeOwned>(user: &str) -> Result<Option<T>, String> {
    match keychain_entry(user)?.get_password() {
        Ok(value) => serde_json::from_str(&value)
            .map(Some)
            .map_err(|error| error.to_string()),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Could not read secure provider settings: {error}")),
    }
}

pub(super) fn write_keychain<T: Serialize>(user: &str, value: &T) -> Result<(), String> {
    let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
    keychain_entry(user)?
        .set_password(&encoded)
        .map_err(|error| format!("Could not save provider settings securely: {error}"))
}

pub(super) fn migrate_legacy_credentials() -> Result<CredentialVault, String> {
    let mut vault = CredentialVault::default();
    for key in [
        PLAID_CREDENTIALS_KEY,
        PLAID_ITEMS_KEY,
        PLAID_USER_KEY,
        SNAPTRADE_CREDENTIALS_KEY,
        ALPACA_CREDENTIALS_KEY,
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

pub(super) fn read_protected_vault() -> Result<Option<CredentialVault>, String> {
    read_keychain(CREDENTIAL_VAULT_KEY)
}

pub(super) fn write_protected_vault(vault: &CredentialVault) -> Result<(), String> {
    write_keychain(CREDENTIAL_VAULT_KEY, vault)
}

pub(super) fn persist_vault_change(
    vault: &mut CredentialVault,
    change: impl FnOnce(&mut CredentialVault) -> Result<(), String>,
    persist: impl FnOnce(&CredentialVault) -> Result<(), String>,
) -> Result<(), String> {
    let mut next = vault.clone();
    change(&mut next)?;
    persist(&next)?;
    *vault = next;
    Ok(())
}

pub(super) fn update_plaid_items(
    vault: &mut CredentialVault,
    change: impl FnOnce(&mut Vec<PlaidItem>),
) -> Result<(), String> {
    let mut items: Vec<PlaidItem> = serde_json::from_value(
        vault
            .entries
            .get(PLAID_ITEMS_KEY)
            .cloned()
            .unwrap_or(json!([])),
    )
    .map_err(|_| "Invalid saved Plaid connections")?;
    change(&mut items);
    vault.entries.insert(
        PLAID_ITEMS_KEY.into(),
        serde_json::to_value(items).map_err(|error| error.to_string())?,
    );
    Ok(())
}

pub(super) fn upsert_plaid_item(
    vault: &mut CredentialVault,
    item: PlaidItem,
) -> Result<(), String> {
    update_plaid_items(vault, |items| {
        if let Some(existing) = items
            .iter_mut()
            .find(|existing| existing.item_id == item.item_id)
        {
            existing.access_token = item.access_token;
        } else {
            items.push(item);
        }
    })
}

pub(super) fn replace_plaid_credentials(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_vault_persistence_does_not_publish_the_mutation() {
        let original = CredentialVault {
            entries: BTreeMap::from([("first".into(), json!("synthetic"))]),
        };
        let mut vault = original.clone();
        assert!(persist_vault_change(
            &mut vault,
            |vault| {
                vault.entries.clear();
                Ok(())
            },
            |_| Err("Persistence failed".into()),
        )
        .is_err());
        assert_eq!(vault.entries, original.entries);
    }

    #[test]
    fn changing_plaid_clients_discards_client_scoped_tokens() {
        let previous = PlaidCredentials {
            client_id: "old-client".into(),
            secret: "old-secret".into(),
        };
        let replacement = PlaidCredentials {
            client_id: "new-client".into(),
            secret: "new-secret".into(),
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
    fn credential_edit_authorization_is_single_use() {
        let providers = Providers::new().expect("provider state");
        assert!(!providers.take_credential_edit_authorization().unwrap());
        providers.authorize_credential_edit().unwrap();
        assert!(providers.take_credential_edit_authorization().unwrap());
        assert!(!providers.take_credential_edit_authorization().unwrap());
    }
}
