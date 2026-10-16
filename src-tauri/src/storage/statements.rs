//! Previewing, importing, and removing statement files. An import changes saved data only
//! through a full offline projection, committed in one transaction like a refresh.
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::Serialize;
use serde_json::Value;

use super::Storage;
use crate::finance_contract::Account;
use crate::statements::{self, Check, Counts, Parsed, ProviderActivity, StatementFile, Status};

/// A chosen file waiting for the user to pick its account. Only Rust holds the bytes.
#[derive(Clone)]
pub struct StatementDraft {
    pub id: String,
    file_name: String,
    contents: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountFit {
    pub account_id: String,
    pub counts: Counts,
    /// The provider's earliest reported activity, before which this statement fills history.
    pub provider_start: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementPreview {
    pub draft_id: String,
    pub file_name: String,
    pub label: Option<String>,
    pub first_date: String,
    pub last_date: String,
    pub rows: usize,
    pub trades: usize,
    pub money_in: f64,
    pub money_out: f64,
    pub check: Check,
    pub dates_assumed: bool,
    pub suggested_account_id: Option<String>,
    /// Set when this exact file is already imported.
    pub imported_account_id: Option<String>,
    /// Accounts this statement can belong to, with what it would add to each.
    pub accounts: Vec<AccountFit>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementSummary {
    pub id: String,
    pub file_name: String,
    pub account_id: String,
    pub imported_at: String,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub rows: usize,
    pub counts: Counts,
    pub check: Option<Check>,
}

/// The rows the user confirmed in an account's saved statements.
fn confirmed_rows(saved: &[(&StatementFile, Parsed)]) -> std::collections::BTreeSet<String> {
    saved
        .iter()
        .flat_map(|(file, _)| file.confirmed_rows.iter().cloned())
        .collect()
}

fn amount(value: Decimal) -> f64 {
    value.round_dp(2).to_f64().unwrap_or_default()
}

/// Words that identify an account, for matching a statement's label to an account name.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| word.len() >= 3 && !word.bytes().all(|byte| byte.is_ascii_digit()))
        .map(str::to_owned)
        .collect()
}

impl Storage {
    fn provider_activity(&self) -> std::collections::BTreeMap<String, Vec<ProviderActivity>> {
        crate::financial_engine::provider_activity(
            &self.data.provider_data.plaid.clone().unwrap_or_default(),
            &self
                .data
                .provider_data
                .snaptrade
                .clone()
                .unwrap_or_default(),
        )
    }

    /// The last four digits each provider reports for an account number.
    fn account_masks(&self) -> std::collections::BTreeMap<String, String> {
        let last_four = |value: &str| {
            let digits = value
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<Vec<_>>();
            (digits.len() >= 4).then(|| digits[digits.len() - 4..].iter().collect::<String>())
        };
        let plaid = self.data.provider_data.plaid.iter().flat_map(|data| {
            data.accounts
                .iter()
                .chain(&data.investment_accounts)
                .filter_map(move |account| {
                    Some((
                        format!("plaid:{}", account["account_id"].as_str()?),
                        last_four(account["mask"].as_str()?)?,
                    ))
                })
        });
        let snaptrade = self.data.provider_data.snaptrade.iter().flat_map(|data| {
            data.accounts.iter().filter_map(move |account| {
                // As `SnapAccount::printed_number`: `number` can be the brokerage's API key.
                let number = account["institution_account_id"]
                    .as_str()
                    .filter(|number| last_four(number).is_some())
                    .or_else(|| account["number"].as_str())
                    .or_else(|| account["account_number"].as_str())?;
                Some((
                    format!("snaptrade:{}", account["id"].as_str()?),
                    last_four(number)?,
                ))
            })
        });
        plaid.chain(snaptrade).collect()
    }

    fn suggested_account(&self, parsed: &Parsed, eligible: &[Account]) -> Option<String> {
        let masks = self.account_masks();
        let label = parsed.hint.label.as_deref().map(words).unwrap_or_default();
        let scored = eligible
            .iter()
            .filter_map(|account| {
                let id = account.id.as_str();
                let name = account.name.as_str();
                let digits_match = parsed.hint.last_four.as_deref().is_some_and(|four| {
                    masks.get(id).is_some_and(|mask| mask == four)
                        || name.trim_end().ends_with(four)
                });
                let shared = words(name)
                    .iter()
                    .filter(|word| label.contains(word))
                    .count();
                let score = usize::from(digits_match) * 100 + shared;
                (score > 0).then_some((score, id))
            })
            .collect::<Vec<_>>();
        let best = scored.iter().map(|(score, _)| *score).max()?;
        match scored
            .iter()
            .filter(|(score, _)| *score == best)
            .collect::<Vec<_>>()[..]
        {
            [(_, id)] => Some((*id).to_owned()),
            _ => None,
        }
    }

    /// Accounts a statement may fill: any account, except that trades need an investment one.
    fn eligible_accounts(&self, parsed: &Parsed) -> Vec<Account> {
        let trades = parsed.has_trades();
        // A manual account's statement values it, so it must be one that holds money.
        let manual = self
            .data
            .settings
            .manual_accounts
            .iter()
            .filter(|manual| matches!(manual.kind.as_str(), "cash" | "brokerage" | "retirement"))
            .map(|manual| Account {
                id: manual.id.clone(),
                name: manual.name.clone(),
                institution: "Manual".into(),
                r#type: manual.kind.clone(),
                ..Account::default()
            });
        self.data
            .snapshot
            .accounts
            .iter()
            .filter(|account| account.id != "all")
            .cloned()
            .chain(manual)
            .filter(|account| {
                !trades || matches!(account.r#type.as_str(), "brokerage" | "retirement")
            })
            .collect()
    }

    /// Saved statements for an account, oldest import first, parsed.
    fn saved_for(&self, account_id: &str) -> Vec<(&StatementFile, Parsed)> {
        let mut saved = self
            .data
            .statements
            .iter()
            .filter(|file| file.account_id == account_id)
            .filter_map(|file| Some((file, statements::parse(&file.contents).ok()?)))
            .collect::<Vec<_>>();
        saved.sort_by(|left, right| {
            (left.0.imported_at.as_str(), left.0.id.as_str())
                .cmp(&(right.0.imported_at.as_str(), right.0.id.as_str()))
        });
        saved
    }

    pub fn preview_statement(
        &self,
        file_name: String,
        contents: Vec<u8>,
    ) -> Result<(StatementPreview, StatementDraft), String> {
        let parsed = statements::parse(&contents)?;
        let id = statements::content_id(&contents);
        let activity = self.provider_activity();
        let eligible = self.eligible_accounts(&parsed);
        let accounts = eligible
            .iter()
            .filter_map(|account| {
                let account_id = account.id.as_str();
                let saved = self.saved_for(account_id);
                let mut rows = saved
                    .iter()
                    .filter(|(file, _)| file.id != id)
                    .map(|(_, parsed)| parsed.rows.as_slice())
                    .collect::<Vec<_>>();
                rows.push(&parsed.rows);
                let confirmed = confirmed_rows(&saved);
                let plan = statements::plan(
                    account_id,
                    &rows,
                    activity
                        .get(account_id)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                    crate::financial_engine::reconciles(account).then_some(&confirmed),
                );
                Some(AccountFit {
                    account_id: account_id.to_owned(),
                    counts: *plan.counts.last()?,
                    provider_start: plan.provider_start.map(|date| date.to_string()),
                })
            })
            .collect();
        let posted = parsed.rows.iter().filter(|row| !row.pending);
        let preview = StatementPreview {
            draft_id: id.clone(),
            file_name: file_name.clone(),
            label: parsed.hint.label.clone(),
            first_date: parsed
                .rows
                .iter()
                .map(|row| row.posted_on)
                .min()
                .map(|date| date.to_string())
                .unwrap_or_default(),
            last_date: parsed
                .rows
                .iter()
                .map(|row| row.posted_on)
                .max()
                .map(|date| date.to_string())
                .unwrap_or_default(),
            rows: parsed.rows.len(),
            trades: parsed
                .rows
                .iter()
                .filter(|row| row.activity.is_trade())
                .count(),
            money_in: amount(
                posted
                    .clone()
                    .map(|row| row.amount)
                    .filter(Decimal::is_sign_positive)
                    .sum(),
            ),
            money_out: amount(
                -posted
                    .map(|row| row.amount)
                    .filter(Decimal::is_sign_negative)
                    .sum::<Decimal>(),
            ),
            check: parsed.check.clone(),
            dates_assumed: parsed.dates_assumed,
            suggested_account_id: self.suggested_account(&parsed, &eligible),
            imported_account_id: self
                .data
                .statements
                .iter()
                .find(|file| file.id == id)
                .map(|file| file.account_id.clone()),
            accounts,
        };
        Ok((
            preview,
            StatementDraft {
                id,
                file_name,
                contents,
            },
        ))
    }

    pub fn import_statement(
        &mut self,
        draft: StatementDraft,
        account_id: &str,
    ) -> Result<Value, String> {
        self.ensure_writable()?;
        let parsed = statements::parse(&draft.contents)?;
        if parsed.check.status == Status::Failed {
            return Err(
                "This statement's total doesn't match its rows, so it wasn't imported".into(),
            );
        }
        if self.data.statements.iter().any(|file| file.id == draft.id) {
            return Err("This statement is already imported".into());
        }
        if self.data.statements.len() >= statements::MAX_FILES {
            return Err("Remove an older statement before importing another".into());
        }
        if !self
            .eligible_accounts(&parsed)
            .iter()
            .any(|account| account.id == account_id)
        {
            return Err(if parsed.has_trades() {
                "Statements with trades belong to an investment account".into()
            } else {
                "Choose an account for this statement".into()
            });
        }
        let file = StatementFile {
            id: draft.id,
            account_id: account_id.to_owned(),
            file_name: draft.file_name,
            imported_at: chrono::Utc::now().to_rfc3339(),
            contents: draft.contents,
            confirmed_rows: Default::default(),
        };
        file.validate()?;
        let mut next = self.data.statements.clone();
        next.push(file);
        self.reproject_cached(next)?;
        self.snapshot()
    }

    /// Imports several statements into one account with a single projection, as a folder of
    /// monthly statements arrives. A statement already imported is matched, not added again.
    /// Returns each draft's key with the id of its import.
    pub fn import_statements(
        &mut self,
        drafts: Vec<(String, StatementDraft)>,
        account_id: &str,
    ) -> Result<Vec<(String, String)>, String> {
        self.ensure_writable()?;
        let mut next = self.data.statements.clone();
        let mut imported = Vec::new();
        for (key, draft) in drafts {
            if next.iter().any(|file| file.id == draft.id) {
                imported.push((key, draft.id));
                continue;
            }
            let parsed = statements::parse(&draft.contents)?;
            if parsed.check.status == Status::Failed {
                return Err(format!(
                    "{}'s total doesn't match its rows, so nothing was added",
                    draft.file_name
                ));
            }
            if !self
                .eligible_accounts(&parsed)
                .iter()
                .any(|account| account.id == account_id)
            {
                return Err(format!("{} can't go into this account", draft.file_name));
            }
            if next.len() >= statements::MAX_FILES {
                return Err("Remove older statements before adding more".into());
            }
            let file = StatementFile {
                id: draft.id.clone(),
                account_id: account_id.to_owned(),
                file_name: draft.file_name,
                imported_at: chrono::Utc::now().to_rfc3339(),
                contents: draft.contents,
                confirmed_rows: Default::default(),
            };
            file.validate()?;
            next.push(file);
            imported.push((key, draft.id));
        }
        if next.len() != self.data.statements.len() {
            self.reproject_cached(next)?;
        }
        Ok(imported)
    }

    pub fn remove_statement(&mut self, id: &str) -> Result<Value, String> {
        self.ensure_writable()?;
        if !self.data.statements.iter().any(|file| file.id == id) {
            return Err("That statement was already removed".into());
        }
        let next = self
            .data
            .statements
            .iter()
            .filter(|file| file.id != id)
            .cloned()
            .collect();
        self.reproject_cached(next)?;
        self.snapshot()
    }

    /// The saved statements with one replaced by a new reading of the same statement, and the
    /// replacement's id. A reading counts as the same statement only when it lists the same
    /// days and amounts in the same order, so only its descriptions change; each confirmed row
    /// stays confirmed. None when nothing changes or the figures differ.
    pub(super) fn reread_statement(
        &self,
        file_id: &str,
        contents: Vec<u8>,
    ) -> Option<(Vec<StatementFile>, String)> {
        let saved = self
            .data
            .statements
            .iter()
            .find(|file| file.id == file_id)?;
        let id = statements::content_id(&contents);
        if id == saved.id {
            return None;
        }
        let (before, after) = (
            statements::parse(&saved.contents).ok()?,
            statements::parse(&contents).ok()?,
        );
        let same = before.rows.len() == after.rows.len()
            && before.rows.iter().zip(&after.rows).all(|(old, new)| {
                (old.posted_on, old.amount, old.pending) == (new.posted_on, new.amount, new.pending)
            });
        if !same || after.check.status == Status::Failed {
            return None;
        }
        let renamed = statements::row_ids(&saved.account_id, &before.rows)
            .into_iter()
            .zip(statements::row_ids(&saved.account_id, &after.rows))
            .filter_map(|(old, new)| old.zip(new))
            .collect::<std::collections::BTreeMap<_, _>>();
        let file = StatementFile {
            id: id.clone(),
            contents,
            confirmed_rows: saved
                .confirmed_rows
                .iter()
                .filter_map(|row| renamed.get(row).cloned())
                .collect(),
            ..saved.clone()
        };
        file.validate().ok()?;
        let next = self
            .data
            .statements
            .iter()
            .filter(|other| other.id != id)
            .map(|other| {
                if other.id == file_id {
                    file.clone()
                } else {
                    other.clone()
                }
            })
            .collect();
        Some((next, id))
    }

    /// Confirms, or withdraws, a statement row Plaid never reported. A confirmed row fills
    /// history until Plaid reports it.
    pub fn confirm_statement_row(
        &mut self,
        file_id: &str,
        row_id: &str,
        confirmed: bool,
    ) -> Result<Value, String> {
        self.ensure_writable()?;
        let awaiting = self
            .data
            .snapshot
            .statement_reconciliation
            .iter()
            .flat_map(|account| &account.missing)
            .any(|row| row.file_id == file_id && row.row_id == row_id);
        if !awaiting {
            return Err("Plaid now reports that row, or its statement was removed".into());
        }
        let mut next = self.data.statements.clone();
        let file = next
            .iter_mut()
            .find(|file| file.id == file_id)
            .ok_or("That statement was removed")?;
        let changed = if confirmed {
            file.confirmed_rows.insert(row_id.to_owned())
        } else {
            file.confirmed_rows.remove(row_id)
        };
        if changed {
            self.reproject_cached(next)?;
        }
        self.snapshot()
    }

    /// Saved statements with what each currently adds, newest import first.
    pub fn statement_summaries(&self) -> Vec<StatementSummary> {
        let activity = self.provider_activity();
        let mut summaries = Vec::new();
        let accounts = self
            .data
            .statements
            .iter()
            .map(|file| file.account_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for account_id in accounts {
            let saved = self.saved_for(account_id);
            let rows = saved
                .iter()
                .map(|(_, parsed)| parsed.rows.as_slice())
                .collect::<Vec<_>>();
            let confirmed = confirmed_rows(&saved);
            let reconciled = self.data.snapshot.accounts.iter().any(|account| {
                account.id == account_id && crate::financial_engine::reconciles(account)
            });
            let plan = statements::plan(
                account_id,
                &rows,
                activity
                    .get(account_id)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                reconciled.then_some(&confirmed),
            );
            for ((file, parsed), counts) in saved.iter().zip(plan.counts) {
                summaries.push(StatementSummary {
                    id: file.id.clone(),
                    file_name: file.file_name.clone(),
                    account_id: file.account_id.clone(),
                    imported_at: file.imported_at.clone(),
                    first_date: parsed
                        .rows
                        .iter()
                        .map(|row| row.posted_on)
                        .min()
                        .map(|date| date.to_string()),
                    last_date: parsed
                        .rows
                        .iter()
                        .map(|row| row.posted_on)
                        .max()
                        .map(|date| date.to_string()),
                    rows: parsed.rows.len(),
                    counts,
                    check: Some(parsed.check.clone()),
                });
            }
        }
        // A file that can no longer be read is still listed, so it can be removed.
        for file in &self.data.statements {
            if !summaries.iter().any(|summary| summary.id == file.id) {
                summaries.push(StatementSummary {
                    id: file.id.clone(),
                    file_name: file.file_name.clone(),
                    account_id: file.account_id.clone(),
                    imported_at: file.imported_at.clone(),
                    first_date: None,
                    last_date: None,
                    rows: 0,
                    counts: Counts::default(),
                    check: None,
                });
            }
        }
        summaries.sort_by(|left, right| {
            right
                .imported_at
                .cmp(&left.imported_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        summaries
    }
}
