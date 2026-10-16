//! Documents in storage: what Brief read is finance state, committed like any edit; the
//! encrypted originals go to `documents.sqlite3` first, so a document never names a file that
//! isn't there.
use std::{collections::BTreeSet, fs, path::PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::Storage;
use crate::documents::{
    crypto::DocumentKey,
    files::{DocumentFiles, FILE_NAME},
    Document, Field, Kind,
};

/// A change to one document. Fields replace the document's own, carrying `edited` for values
/// the user typed.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct DocumentChange {
    pub kind: Option<Kind>,
    pub form: Option<String>,
    pub issuer: Option<String>,
    pub tax_year: Option<i32>,
    /// An empty id clears the account.
    pub account_id: Option<String>,
    /// An empty id clears the person.
    pub person: Option<String>,
    pub corrected: Option<bool>,
    pub reviewed: Option<bool>,
    pub fields: Option<Vec<Field>>,
}

impl Storage {
    pub fn documents_path(&self) -> Result<PathBuf, String> {
        self.path
            .parent()
            .map(|directory| directory.join(FILE_NAME))
            .ok_or_else(|| "Local data directory unavailable".into())
    }

    fn document_files(&self) -> Result<DocumentFiles, String> {
        DocumentFiles::open(&self.documents_path()?)
    }

    fn commit_documents(&mut self, documents: Vec<Document>) -> Result<(), String> {
        for document in &documents {
            document.validate()?;
        }
        self.backup_before_edit()?;
        self.database.replace_documents(&documents)?;
        self.data.documents = documents;
        Ok(())
    }

    /// Saves new documents and their sealed originals. A document already saved is left as it
    /// is.
    pub fn add_documents(
        &mut self,
        added: Vec<(Document, Vec<u8>)>,
        key: &DocumentKey,
    ) -> Result<Value, String> {
        self.ensure_writable()?;
        let files = self.document_files()?;
        let mut next = self.data.documents.clone();
        for (document, sealed) in added {
            if next.iter().any(|known| known.id == document.id) {
                continue;
            }
            files.put(&document.id, &key.fingerprint(), &sealed)?;
            next.push(document);
        }
        next.sort_by(|left, right| {
            left.imported_at
                .cmp(&right.imported_at)
                .then(left.id.cmp(&right.id))
        });
        self.commit_documents(next)?;
        self.snapshot()
    }

    pub fn update_document(&mut self, id: &str, change: DocumentChange) -> Result<Value, String> {
        self.ensure_writable()?;
        let mut next = self.data.documents.clone();
        let document = next
            .iter_mut()
            .find(|document| document.id == id)
            .ok_or("That document was removed")?;
        let text = |value: String| {
            let value = value.trim().to_owned();
            (!value.is_empty()).then_some(value)
        };
        if let Some(kind) = change.kind {
            document.kind = kind;
        }
        if let Some(form) = change.form {
            document.form = text(form);
        }
        if let Some(issuer) = change.issuer {
            document.issuer = text(issuer);
        }
        if let Some(year) = change.tax_year {
            if !(1990..=2100).contains(&year) {
                return Err("Enter a year such as 2026".into());
            }
            document.tax_year = Some(year);
        }
        if let Some(account_id) = change.account_id {
            let account_id = text(account_id);
            if account_id.as_ref().is_some_and(|id| {
                !self
                    .data
                    .snapshot
                    .accounts
                    .iter()
                    .any(|account| &account.id == id)
                    && !self
                        .data
                        .settings
                        .manual_accounts
                        .iter()
                        .any(|account| &account.id == id)
            }) {
                return Err("That account isn't available".into());
            }
            document.account_id = account_id;
        }
        if let Some(person) = change.person {
            document.person = text(person);
        }
        if let Some(corrected) = change.corrected {
            document.corrected = corrected;
        }
        if let Some(reviewed) = change.reviewed {
            document.reviewed = reviewed;
        }
        if let Some(fields) = change.fields {
            document.fields = fields;
        }
        self.commit_documents(next)?;
        self.snapshot()
    }

    /// Removes documents and their encrypted originals. History a statement added stays until
    /// the user removes that import.
    pub fn delete_documents(&mut self, ids: &[String]) -> Result<Value, String> {
        self.ensure_writable()?;
        let next = self
            .data
            .documents
            .iter()
            .filter(|document| !ids.contains(&document.id))
            .cloned()
            .collect::<Vec<_>>();
        self.commit_documents(next)?;
        self.prune_document_files()?;
        self.snapshot()
    }

    /// Drops originals no document names, such as one deleted or one a restore left out.
    pub fn prune_document_files(&self) -> Result<(), String> {
        if !self.documents_path()?.exists() {
            return Ok(());
        }
        let keep = self
            .data
            .documents
            .iter()
            .map(|document| document.id.as_str())
            .collect::<BTreeSet<_>>();
        self.document_files()?.keep_only(&keep)
    }

    /// A document's original bytes, decrypted with this Mac's key.
    pub fn document_contents(&self, id: &str, key: &DocumentKey) -> Result<Vec<u8>, String> {
        let (sealed, fingerprint) = self
            .document_files()?
            .get(id)?
            .ok_or("This document's original isn't saved on this Mac")?;
        if fingerprint != key.fingerprint() {
            return Err("This document was saved with another Mac's key".into());
        }
        key.open(id, &sealed)
    }

    /// Writes a document's original to a private file, replacing the copy opened before, so
    /// the Mac's own viewer can show it.
    pub fn open_document_copy(&self, id: &str, key: &DocumentKey) -> Result<PathBuf, String> {
        let document = self
            .data
            .documents
            .iter()
            .find(|document| document.id == id)
            .ok_or("That document was removed")?;
        let contents = self.document_contents(id, key)?;
        let directory = self.opened_directory()?;
        if directory.exists() {
            fs::remove_dir_all(&directory).map_err(|_| "Could not clear the previous document")?;
        }
        fs::create_dir_all(&directory).map_err(|_| "Could not prepare the document")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| "Could not prepare the document")?;
        }
        let name = document
            .file_name
            .chars()
            .filter(|c| !matches!(c, '/' | ':' | '\\') && !c.is_control())
            .collect::<String>();
        let path = directory.join(if name.trim().is_empty() {
            "Document".into()
        } else {
            name
        });
        crate::files::write_private(&path, &contents, "Could not prepare the document")?;
        Ok(path)
    }

    /// Replaces a document's reading with a new one, keeping every value the user typed and
    /// whatever they set by hand.
    pub fn replace_reading(&mut self, read: Document) -> Result<Value, String> {
        self.ensure_writable()?;
        let mut next = self.data.documents.clone();
        let document = next
            .iter_mut()
            .find(|document| document.id == read.id)
            .ok_or("That document was removed")?;
        let fields = crate::documents::merge_fields(&document.fields, read.fields);
        // A statement already in history takes the new reading's rows when only their
        // descriptions changed; otherwise its rows stay until the user adds it again.
        let reread = document
            .statement_file_id
            .as_deref()
            .zip(read.statement_csv.clone())
            .and_then(|(file_id, csv)| self.reread_statement(file_id, csv.into_bytes()));
        *document = Document {
            fields,
            account_id: document.account_id.clone().or(read.account_id),
            person: document.person.clone(),
            reviewed: document.reviewed,
            statement_file_id: reread
                .as_ref()
                .map(|(_, id)| id.clone())
                .or_else(|| document.statement_file_id.clone()),
            imported_at: document.imported_at.clone(),
            // The file is unchanged, so what it says is too.
            summary: document.summary.clone(),
            ..read
        };
        if let Some((statements, _)) = reread {
            self.reproject_cached(statements)?;
        }
        self.commit_documents(next)?;
        self.snapshot()
    }

    /// Saves a document's summary. A document removed meanwhile is left alone.
    pub fn set_document_summary(&mut self, id: &str, summary: String) -> Result<(), String> {
        self.ensure_writable()?;
        let mut next = self.data.documents.clone();
        let Some(document) = next.iter_mut().find(|document| document.id == id) else {
            return Ok(());
        };
        document.summary = Some(summary);
        self.commit_documents(next)
    }

    /// Adds the history in statement documents to one account, through the statement importer
    /// so its checks and reconciliation apply. Each document then names the import it made.
    pub fn add_document_history(
        &mut self,
        ids: &[String],
        account_id: &str,
        key: &DocumentKey,
    ) -> Result<Value, String> {
        self.ensure_writable()?;
        let mut files = Vec::new();
        for id in ids {
            let document = self
                .data
                .documents
                .iter()
                .find(|document| &document.id == id)
                .ok_or("That document was removed")?;
            if document.statement_file_id.is_some() {
                continue;
            }
            let contents = match (&document.statement_csv, document.kind) {
                (Some(csv), _) => csv.clone().into_bytes(),
                (None, Kind::Export) => self.document_contents(id, key)?,
                _ => {
                    return Err(format!(
                        "{} has no history Brief can add",
                        document.file_name
                    ))
                }
            };
            let (_, draft) = self.preview_statement(document.file_name.clone(), contents)?;
            files.push((id.clone(), draft));
        }
        let imported = self.import_statements(files, account_id)?;
        let mut next = self.data.documents.clone();
        for document in &mut next {
            if let Some((_, file_id)) = imported.iter().find(|(id, _)| *id == document.id) {
                document.statement_file_id = Some(file_id.clone());
                document.account_id = Some(account_id.to_owned());
            }
        }
        self.commit_documents(next)?;
        self.snapshot()
    }
}
