//! The log of what Brief looks up on the web: a card's published terms, through Exa, only when
//! research is on. Each query is logged before it runs and completed after, with the pages that
//! came back, so the log is exactly what left the Mac. A terms page is read again monthly, and a
//! changed page flags the rules drawn from it for review.
//!
//! Pages are data, never instructions: what comes back is shown, never acted on by itself.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::providers::exa::Page;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchEntryKind")]
pub enum EntryKind {
    /// A search for a card's terms page.
    Search,
    /// A page read by its address, such as a card's terms.
    Recheck,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchStatus")]
pub enum Status {
    Running,
    Done,
    Failed,
}

/// One logged query: exactly what left the Mac, and what came back.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchEntry")]
pub struct Entry {
    pub id: String,
    pub kind: EntryKind,
    /// The query text, or the URL read.
    pub query: String,
    /// What it was for, such as a catalog card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subject: Option<String>,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub finished_at: Option<String>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    pub pages: Vec<Page>,
}

impl Entry {
    pub fn begin(
        kind: EntryKind,
        query: String,
        subject: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        let started_at = now.to_rfc3339();
        Self {
            id: crate::database::sha256(format!("{started_at}\n{query}").as_bytes())[..16]
                .to_owned(),
            kind,
            query,
            subject,
            started_at,
            finished_at: None,
            status: Status::Running,
            error: None,
            pages: Vec::new(),
        }
    }

    pub fn finish(&mut self, result: Result<Vec<Page>, String>, now: DateTime<Utc>) {
        self.finished_at = Some(now.to_rfc3339());
        match result {
            Ok(pages) => {
                self.status = Status::Done;
                self.pages = pages;
            }
            Err(error) => {
                self.status = Status::Failed;
                self.error = Some(error);
            }
        }
    }
}

/// What a terms page said when last read.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SourceCheck")]
pub struct SourceCheck {
    pub url: String,
    /// SHA-256 of the page's text, with spacing and case evened out.
    pub hash: String,
    pub checked_at: String,
    /// When a read found the page different from the one before; cleared once reviewed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub changed_at: Option<String>,
}

/// How often each terms page is read again.
pub const RECHECK_DAYS: i64 = 30;

/// The page's text as compared between reads.
pub fn page_hash(text: &str) -> String {
    let even = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    crate::database::sha256(even.as_bytes())
}

/// Whether a page is due to be read again.
pub fn due(check: Option<&SourceCheck>, now: DateTime<Utc>) -> bool {
    check
        .and_then(|check| DateTime::parse_from_rfc3339(&check.checked_at).ok())
        .is_none_or(|checked| (now - checked.to_utc()).num_days() >= RECHECK_DAYS)
}

/// The check after reading a page: the first read records it; a later read that differs marks
/// it changed, and a mark stays until the user reviews it.
pub fn checked(
    previous: Option<&SourceCheck>,
    url: &str,
    text: &str,
    now: DateTime<Utc>,
) -> SourceCheck {
    let hash = page_hash(text);
    let changed_at = match previous {
        Some(previous) if previous.hash != hash => Some(now.to_rfc3339()),
        Some(previous) => previous.changed_at.clone(),
        None => None,
    };
    SourceCheck {
        url: url.to_owned(),
        hash,
        checked_at: now.to_rfc3339(),
        changed_at,
    }
}

#[cfg(test)]
mod tests;
