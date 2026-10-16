//! Plaid Statements: the PDF statements a bank issued for an item's checking and savings
//! accounts, exactly as the bank produced them. Only items the user added statements to through
//! Link are read. Brief stores the files; it does not read figures from them.
use sha2::{Digest, Sha256};

use super::*;

/// The largest statement Brief accepts.
pub const MAX_STATEMENT_BYTES: usize = 25 * 1024 * 1024;

/// One statement Plaid can download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedStatement {
    pub statement_id: String,
    pub item_id: String,
    /// The Brief account ID, `plaid:` and Plaid's account ID.
    pub account_id: String,
    pub account_name: String,
    pub year: i32,
    pub month: u32,
    pub date_posted: Option<String>,
}

impl Providers {
    /// The bank items the user added statements to.
    pub fn statement_items(&self) -> Result<Vec<String>, String> {
        Ok(self
            .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
            .unwrap_or_default()
            .into_iter()
            .filter(|item| item.statements && !item.investments)
            .map(|item| item.item_id)
            .collect())
    }

    fn statements_token(&self, item_id: &str) -> Result<String, String> {
        self.read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
            .unwrap_or_default()
            .into_iter()
            .find(|item| item.item_id == item_id && item.statements)
            .map(|item| item.access_token)
            .ok_or_else(|| "Statements aren't added for that connection".into())
    }

    /// The statements Plaid holds for `item_id`.
    pub async fn list_statements(&self, item_id: &str) -> Result<Vec<ListedStatement>, String> {
        let credentials = self.plaid_credentials()?;
        let access_token = self.statements_token(item_id)?;
        let (response, _) = self
            .plaid_read(
                &credentials,
                "/statements/list",
                json!({ "access_token": access_token }),
            )
            .await
            .map_err(|error| error.to_string())?;
        listed_statements(item_id, &response)
    }

    /// Asks Plaid to fetch statements posted between `start` and `end` from the bank. Plaid
    /// fetches them in the background; a later list includes them.
    pub async fn request_statements(
        &self,
        item_id: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<(), String> {
        let credentials = self.plaid_credentials()?;
        let access_token = self.statements_token(item_id)?;
        self.plaid_once(
            &credentials,
            "/statements/refresh",
            json!({
                "access_token": access_token,
                "start_date": start.to_string(),
                "end_date": end.to_string(),
            }),
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    /// One statement's PDF, checked against the SHA-256 checksum Plaid sends with it.
    pub async fn download_statement(
        &self,
        item_id: &str,
        statement_id: &str,
    ) -> Result<Vec<u8>, String> {
        const ENDPOINT: &str = "/statements/download";
        let credentials = self.plaid_credentials()?;
        let access_token = self.statements_token(item_id)?;
        http::count_request("Plaid", false);
        let response = self
            .http
            .post(format!("https://production.plaid.com{ENDPOINT}"))
            .header("PLAID-CLIENT-ID", &credentials.client_id)
            .header("PLAID-SECRET", &credentials.secret)
            .json(&json!({ "access_token": access_token, "statement_id": statement_id }))
            .send()
            .await
            .map_err(|_| http::ProviderError::network("Plaid", ENDPOINT, 1).to_string())?;
        if !response.status().is_success() {
            return Err(
                match http::ProviderError::from_response("Plaid", ENDPOINT, response, 1).await {
                    Err(error) => error.to_string(),
                    Ok(_) => "Plaid could not send that statement".into(),
                },
            );
        }
        let checksum = response
            .headers()
            .get("Plaid-Content-Hash")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let contents = http::response_bytes(response, MAX_STATEMENT_BYTES).await?;
        http::count_bytes("Plaid", contents.len());
        verified_pdf(contents, checksum.as_deref())
    }
}

/// The statements in a `/statements/list` response, for the accounts it names.
pub(super) fn listed_statements(
    item_id: &str,
    response: &Value,
) -> Result<Vec<ListedStatement>, String> {
    let accounts = response
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or("Plaid's statement list had no accounts")?;
    let mut listed = Vec::new();
    for account in accounts {
        let Some(account_id) = account.get("account_id").and_then(Value::as_str) else {
            continue;
        };
        let name = ["account_name", "account_official_name"]
            .into_iter()
            .filter_map(|field| account.get(field).and_then(Value::as_str))
            .find(|name| !name.trim().is_empty())
            .unwrap_or("Account");
        for statement in account
            .get("statements")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let statement_id = statement.get("statement_id").and_then(Value::as_str);
            let year = statement.get("year").and_then(Value::as_i64);
            let month = statement.get("month").and_then(Value::as_u64);
            let (Some(statement_id), Some(year), Some(month)) = (statement_id, year, month) else {
                continue;
            };
            if statement_id.is_empty()
                || statement_id.len() > 128
                || !(2010..=2100).contains(&year)
                || !(1..=12).contains(&month)
            {
                continue;
            }
            listed.push(ListedStatement {
                statement_id: statement_id.into(),
                item_id: item_id.into(),
                account_id: format!("plaid:{account_id}"),
                account_name: name.chars().take(120).collect(),
                year: year as i32,
                month: month as u32,
                date_posted: statement
                    .get("date_posted")
                    .and_then(Value::as_str)
                    .filter(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok())
                    .map(str::to_owned),
            });
        }
    }
    Ok(listed)
}

/// `contents` when it is a PDF whose SHA-256 matches `checksum`, the hex digest Plaid sends.
pub(super) fn verified_pdf(contents: Vec<u8>, checksum: Option<&str>) -> Result<Vec<u8>, String> {
    if !contents.starts_with(b"%PDF-") {
        return Err("Plaid sent a statement that isn't a PDF".into());
    }
    let checksum = checksum.ok_or("Plaid sent a statement without its checksum")?;
    let digest = Sha256::digest(&contents)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if !digest.eq_ignore_ascii_case(checksum.trim()) {
        return Err("A statement didn't match Plaid's checksum; it wasn't saved".into());
    }
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_are_listed_by_account_and_malformed_entries_are_skipped() {
        let listed = listed_statements(
            "item-synthetic",
            &json!({
                "accounts": [{
                    "account_id": "checking-synthetic",
                    "account_name": "Synthetic Checking",
                    "statements": [
                        { "statement_id": "s-1", "year": 2026, "month": 8, "date_posted": "2026-08-31" },
                        { "statement_id": "s-2", "year": 2026, "month": 13 },
                        { "statement_id": "", "year": 2026, "month": 7 },
                        { "statement_id": "s-3", "year": 2026, "month": 7, "date_posted": "July" }
                    ]
                }]
            }),
        )
        .unwrap();
        assert_eq!(
            listed,
            vec![
                ListedStatement {
                    statement_id: "s-1".into(),
                    item_id: "item-synthetic".into(),
                    account_id: "plaid:checking-synthetic".into(),
                    account_name: "Synthetic Checking".into(),
                    year: 2026,
                    month: 8,
                    date_posted: Some("2026-08-31".into()),
                },
                ListedStatement {
                    statement_id: "s-3".into(),
                    item_id: "item-synthetic".into(),
                    account_id: "plaid:checking-synthetic".into(),
                    account_name: "Synthetic Checking".into(),
                    year: 2026,
                    month: 7,
                    date_posted: None,
                },
            ]
        );
        assert!(listed_statements("item", &json!({})).is_err());
    }

    #[test]
    fn only_a_pdf_matching_its_checksum_is_kept() {
        let pdf = b"%PDF-1.7 synthetic".to_vec();
        let checksum = Sha256::digest(&pdf)
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        assert_eq!(verified_pdf(pdf.clone(), Some(&checksum)).unwrap(), pdf);
        assert!(verified_pdf(pdf.clone(), Some("00")).is_err());
        assert!(verified_pdf(pdf, None).is_err());
        assert!(verified_pdf(b"<html>".to_vec(), Some(&checksum)).is_err());
    }

    #[test]
    fn the_statements_link_adds_two_years_to_the_saved_item() {
        let request = plaid_statements_link_request(
            "access-synthetic".into(),
            NaiveDate::from_ymd_opt(2026, 10, 12).unwrap(),
        );
        assert_eq!(request["access_token"], "access-synthetic");
        assert_eq!(request["products"], json!(["statements"]));
        assert_eq!(request["statements"]["start_date"], "2024-10-12");
        assert_eq!(request["statements"]["end_date"], "2026-10-12");
        assert!(request.get("enable_multi_item_link").is_none());
    }
}
