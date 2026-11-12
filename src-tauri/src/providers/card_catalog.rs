//! The open card benefits catalog (us-credit-card-benefits, MIT): every card's recurring
//! statement credits, downloaded whole on request so the download never says which card the
//! user holds. Parsed tolerantly, since the catalog is maintained by others.
use super::*;

pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/tiepnv2508/us-credit-card-benefits/main/exports/benefits.json";
pub const CATALOG_HOME: &str = "https://github.com/tiepnv2508/us-credit-card-benefits";
const MAX_CATALOG_BYTES: usize = 3 * 1024 * 1024;
/// The catalog changes about monthly; a day's copy is current enough.
const CATALOG_KEPT: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogBenefit {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub frequency: Option<String>,
    #[serde(default)]
    pub interval_months: Option<u32>,
    #[serde(default)]
    pub alignment: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub in_app: bool,
    #[serde(default)]
    pub enrollment_required: bool,
    #[serde(default)]
    pub match_keywords: Vec<String>,
    #[serde(default)]
    pub match_verified: bool,
    #[serde(default)]
    pub descriptor_examples: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCardEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub issuer: String,
    #[serde(default)]
    pub is_business: bool,
    #[serde(default)]
    pub annual_fee: Option<f64>,
    #[serde(default)]
    pub discontinued: bool,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub last_verified: Option<String>,
    #[serde(default)]
    pub benefits: Vec<CatalogBenefit>,
    /// SHA-256 of the card's entry as published, to tell when it changes.
    #[serde(skip)]
    pub hash: String,
}

#[derive(Clone, Debug, Default)]
pub struct CardCatalog {
    pub cards: Vec<CatalogCardEntry>,
}

impl CardCatalog {
    pub fn card(&self, id: &str) -> Option<&CatalogCardEntry> {
        self.cards.iter().find(|card| card.id == id)
    }
}

/// The catalog from its published JSON. A card that can't be read is skipped, not fatal.
pub fn parse_catalog(body: &[u8]) -> Result<CardCatalog, String> {
    let root: Value =
        serde_json::from_slice(body).map_err(|_| "The card catalog isn't readable JSON")?;
    let cards = root
        .get("cards")
        .and_then(Value::as_array)
        .ok_or("The card catalog has no cards")?;
    Ok(CardCatalog {
        cards: cards
            .iter()
            .filter_map(|raw| {
                let mut card: CatalogCardEntry = serde_json::from_value(raw.clone()).ok()?;
                (!card.id.is_empty() && !card.name.is_empty()).then(|| {
                    card.hash = crate::database::sha256(raw.to_string().as_bytes());
                    card
                })
            })
            .collect(),
    })
}

impl Providers {
    /// The catalog, downloaded at most once a day unless `fresh`.
    pub async fn card_catalog(&self, fresh: bool) -> Result<std::sync::Arc<CardCatalog>, String> {
        let mut cached = self.card_catalog.lock().await;
        if let (false, Some((fetched, catalog))) = (fresh, cached.as_ref()) {
            if fetched.elapsed() < CATALOG_KEPT {
                return Ok(catalog.clone());
            }
        }
        let response = self
            .http
            .get(CATALOG_URL)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
            .map_err(|_| "Could not reach the card catalog")?;
        if !response.status().is_success() {
            return Err(format!("The card catalog returned {}", response.status()));
        }
        let body = http::response_bytes(response, MAX_CATALOG_BYTES).await?;
        let catalog = std::sync::Arc::new(parse_catalog(&body)?);
        *cached = Some((std::time::Instant::now(), catalog.clone()));
        Ok(catalog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catalog_reads_tolerantly_and_skips_cards_it_cannot_read() {
        let body = br#"{
            "schemaVersion": 2,
            "cards": [
                { "id": "test-card", "name": "Test Card", "issuer": "Test Bank", "unknown": 1,
                  "benefits": [{ "id": "streaming", "name": "Streaming", "type": "credit",
                                 "value": 10, "frequency": "monthly", "extra": true }] },
                { "id": "", "name": "No id" },
                "not a card"
            ]
        }"#;
        let catalog = parse_catalog(body).unwrap();
        assert_eq!(catalog.cards.len(), 1);
        let card = catalog.card("test-card").unwrap();
        assert_eq!(card.benefits[0].value, Some(10.0));
        assert_eq!(card.hash.len(), 64);
        assert!(parse_catalog(b"{}").is_err());
    }
}
