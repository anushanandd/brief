//! Bank statement PDFs in the finance database, and the private copies opened to read them.
use std::{fs, path::PathBuf};

use super::Storage;
use crate::database::{BankStatement, StatementCheck};

/// Where a statement is copied to open it; cleared before each copy and by erasure.
pub(crate) const OPENED_DIRECTORY: &str = "opened-statements";

impl Storage {
    pub fn bank_statements(&self) -> Result<Vec<BankStatement>, String> {
        self.database.bank_statements()
    }

    pub fn bank_statement_ids(&self) -> Result<std::collections::BTreeSet<String>, String> {
        Ok(self
            .database
            .bank_statements()?
            .into_iter()
            .map(|statement| statement.id)
            .collect())
    }

    pub fn save_bank_statement(
        &mut self,
        statement: &BankStatement,
        sha256: &str,
        contents: &[u8],
    ) -> Result<(), String> {
        self.ensure_writable()?;
        self.database
            .save_bank_statement(statement, sha256, contents)
    }

    pub fn statement_check(&self, item_id: &str) -> Result<StatementCheck, String> {
        self.database.statement_check(item_id)
    }

    pub fn record_statement_check(
        &mut self,
        item_id: &str,
        check: &StatementCheck,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        self.database.record_statement_check(item_id, check)
    }

    /// Copies one statement to a new private file in the app's data directory, replacing the
    /// copy opened before, and returns its path.
    pub fn open_copy(&self, id: &str) -> Result<PathBuf, String> {
        let statement = self
            .database
            .bank_statements()?
            .into_iter()
            .find(|statement| statement.id == id)
            .ok_or("That statement isn't saved")?;
        let contents = self
            .database
            .bank_statement_contents(id)?
            .ok_or("That statement isn't saved")?;
        let directory = self.opened_directory()?;
        if directory.exists() {
            fs::remove_dir_all(&directory).map_err(|_| "Could not clear the previous statement")?;
        }
        fs::create_dir_all(&directory).map_err(|_| "Could not prepare the statement")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| "Could not prepare the statement")?;
        }
        let path = directory.join(file_name(&statement));
        crate::files::write_private(&path, &contents, "Could not prepare the statement")?;
        Ok(path)
    }

    pub(super) fn opened_directory(&self) -> Result<PathBuf, String> {
        self.path
            .parent()
            .map(|directory| directory.join(OPENED_DIRECTORY))
            .ok_or_else(|| "Local data directory unavailable".into())
    }
}

/// `Checking 2026-08.pdf`: the account's name, kept to letters, digits, and spaces, and the
/// statement's month.
fn file_name(statement: &BankStatement) -> String {
    let name = statement
        .account_name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == ' ')
        .take(60)
        .collect::<String>();
    let name = name.trim();
    format!(
        "{} {}-{:02}.pdf",
        if name.is_empty() { "Statement" } else { name },
        statement.year,
        statement.month
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opened_copies_are_named_by_account_and_month_and_replace_the_last_one() {
        let directory = std::env::temp_dir().join(format!("brief-opened-{}", uuid::Uuid::new_v4()));
        let mut storage = Storage::new(directory.clone()).unwrap();
        let statement = |id: &str, month: u32| BankStatement {
            id: id.into(),
            item_id: "item".into(),
            account_id: "plaid:checking".into(),
            account_name: "Everyday Checking / ../".into(),
            year: 2026,
            month,
            date_posted: None,
            fetched_at: "2026-10-12T12:00:00Z".into(),
            bytes: 0,
        };
        let digest = "b".repeat(64);
        storage
            .save_bank_statement(&statement("july", 7), &digest, b"%PDF-july")
            .unwrap();
        storage
            .save_bank_statement(&statement("august", 8), &digest, b"%PDF-august")
            .unwrap();

        let july = storage.open_copy("july").unwrap();
        assert!(july.ends_with("opened-statements/Everyday Checking 2026-07.pdf"));
        assert_eq!(fs::read(&july).unwrap(), b"%PDF-july");
        let august = storage.open_copy("august").unwrap();
        assert!(!july.exists());
        assert_eq!(fs::read(&august).unwrap(), b"%PDF-august");
        assert!(storage.open_copy("missing").is_err());

        storage.erase().unwrap();
        assert!(storage.bank_statements().unwrap().is_empty());
        assert!(!august.exists());
        drop(storage);
        fs::remove_dir_all(directory).unwrap();
    }
}
