use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use std::io::Write;

use crate::{
    database::{Database, SyncRun},
    providers::{PlaidCache, ProviderDataCache, ProviderSync},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceState {
    pub(crate) schema_version: u32,
    pub revision: u64,
    pub snapshot: Value,
    pub plaid_cache: PlaidCache,
    pub provider_data: ProviderDataCache,
    #[serde(default)]
    pub annotations: BTreeMap<String, Annotation>,
    #[serde(default)]
    pub account_links: BTreeMap<String, String>,
}

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Annotation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reviewed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) benefit_confirmed: Option<bool>,
}

struct PendingRefresh {
    id: String,
    started_at: String,
    revision: u64,
    expires: Instant,
    plaid_cache: PlaidCache,
    provider_data: ProviderDataCache,
    warnings: Vec<String>,
}

pub struct Storage {
    _process_lock: fs::File,
    path: PathBuf,
    database: Database,
    pub data: FinanceState,
    pending: Option<PendingRefresh>,
    recovery: Option<Recovery>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Recovery {
    message: String,
    can_restore: bool,
}

pub(crate) fn empty_state() -> Result<FinanceState, String> {
    Ok(FinanceState {
        schema_version: 1,
        revision: 0,
        snapshot: serde_json::from_str(include_str!("../../src/data/empty.json"))
            .map_err(|error| error.to_string())?,
        plaid_cache: PlaidCache::default(),
        provider_data: ProviderDataCache::default(),
        annotations: BTreeMap::new(),
        account_links: BTreeMap::new(),
    })
}

fn validate_state(data: FinanceState) -> Result<FinanceState, String> {
    if data.schema_version != 1 {
        return Err("This financial data requires a newer version of Brief".into());
    }
    let mut linked_snaptrade_accounts = BTreeSet::new();
    if data.account_links.iter().any(|(plaid, snaptrade)| {
        !plaid.starts_with("plaid:")
            || !snaptrade.starts_with("snaptrade:")
            || plaid.len() > 220
            || snaptrade.len() > 220
            || !linked_snaptrade_accounts.insert(snaptrade)
    }) {
        return Err("Invalid canonical account links".into());
    }
    validate_snapshot(&data.snapshot)?;
    Ok(data)
}

fn load_database(path: &Path) -> Result<(Database, FinanceState), String> {
    let database = Database::open(path)?;
    let data = database
        .load()?
        .ok_or_else(|| "The finance database has no committed state".to_string())?;
    Ok((database, validate_state(data)?))
}

fn legacy_state(directory: &Path) -> Result<FinanceState, String> {
    let state_path = directory.join("finance-state.json");
    if state_path.exists() {
        return read(&state_path);
    }
    let snapshot_path = directory.join("finance-snapshot.json");
    Ok(FinanceState {
        schema_version: 1,
        revision: 0,
        snapshot: if snapshot_path.exists() {
            read(&snapshot_path)?
        } else {
            serde_json::from_str(include_str!("../../src/data/empty.json"))
                .map_err(|error| error.to_string())?
        },
        plaid_cache: read_or_default(&directory.join("plaid-cache.json"))?,
        provider_data: read_or_default(&directory.join("provider-data.json"))?,
        annotations: BTreeMap::new(),
        account_links: BTreeMap::new(),
    })
}

impl Storage {
    pub fn cancel_pending_refresh(&mut self) {
        self.pending = None;
    }

    pub fn sync_runs(&self) -> Result<Vec<SyncRun>, String> {
        self.database.sync_runs()
    }

    pub fn record_failed_sync(&mut self, id: &str, started_at: &str, error_code: &str) {
        let _ = self.database.record_sync_run(
            id,
            started_at,
            &chrono::Utc::now().to_rfc3339(),
            "failed",
            &[],
            Some(error_code),
        );
    }

    pub fn snapshot(&self) -> Result<Value, String> {
        let mut snapshot = crate::financial_engine::apply_annotations(
            &self.data.snapshot,
            &self.data.annotations,
        )?;
        if let Some(recovery) = &self.recovery {
            snapshot["recovery"] = serde_json::to_value(recovery).unwrap_or(Value::Null);
        }
        Ok(snapshot)
    }

    pub fn upgrade_projection(&mut self) -> Result<bool, String> {
        let version = self.data.snapshot["calculationVersion"].as_u64();
        if self.recovery.is_some()
            || self.pending.is_some()
            || version.is_some_and(|value| {
                value >= u64::from(crate::financial_engine::CALCULATION_VERSION)
            })
            || (self.data.provider_data.plaid.is_none()
                && self.data.provider_data.snaptrade.is_none())
        {
            return Ok(false);
        }

        let previous = self.data.snapshot.clone();
        let now = previous["updatedAt"]
            .as_str()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);
        let sync = ProviderSync {
            sync_id: "cached-projection-upgrade".into(),
            balances_fresh: false,
            refreshed_account_ids: Vec::new(),
            previous_snapshot: previous.clone(),
            plaid: self.data.provider_data.plaid.clone().unwrap_or_default(),
            snaptrade: self
                .data
                .provider_data
                .snaptrade
                .clone()
                .unwrap_or_default(),
            benchmark_history: previous
                .get("benchmarkHistory")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
            security_history: self.data.provider_data.security_history.clone(),
        };
        let mut snapshot = crate::financial_engine::project(&sync, &self.data.account_links, now)?;
        let revision = self
            .data
            .revision
            .checked_add(1)
            .ok_or("Snapshot revision overflow")?;
        snapshot["revision"] = revision.into();
        snapshot["providerStatus"] = serde_json::to_value(&self.data.provider_data.sync_status)
            .map_err(|error| error.to_string())?;
        if snapshot.get("lastChange").is_none() {
            if let Some(change) = previous.get("lastChange") {
                snapshot["lastChange"] = change.clone();
            }
        }
        let mut warnings = previous
            .get("syncWarnings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .chain(
                snapshot
                    .get("syncWarnings")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            )
            .collect::<Vec<_>>();
        warnings.sort();
        warnings.dedup();
        snapshot["syncWarnings"] =
            serde_json::to_value(warnings).map_err(|error| error.to_string())?;

        let next = validate_state(FinanceState {
            schema_version: self.data.schema_version,
            revision,
            snapshot,
            plaid_cache: self.data.plaid_cache.clone(),
            provider_data: self.data.provider_data.clone(),
            annotations: self.data.annotations.clone(),
            account_links: self.data.account_links.clone(),
        })?;
        self.database.backup(&self.backup_path())?;
        self.database.replace(&next)?;
        self.data = next;
        Ok(true)
    }

    pub fn ensure_writable(&self) -> Result<(), String> {
        if self.recovery.is_some() {
            Err("Restore or start a new local snapshot before changing saved data".into())
        } else {
            Ok(())
        }
    }

    fn backup_path(&self) -> PathBuf {
        self.path.with_file_name("finance-state.backup.sqlite3")
    }

    pub fn recover(&mut self, restore: bool) -> Result<Value, String> {
        let recovery = self.recovery.as_ref().ok_or("No recovery is needed")?;
        if restore && !recovery.can_restore {
            return Err("No valid retained snapshot is available".into());
        }
        let next = validate_state(if restore {
            self.data.clone()
        } else {
            empty_state()?
        })?;
        // Retain the original bytes before replacing anything, including a newer-version file.
        for path in [&self.path, &self.backup_path()] {
            if path.exists() {
                let retained =
                    path.with_extension(format!("retained-{}.sqlite3", uuid::Uuid::new_v4()));
                fs::copy(path, retained)
                    .map_err(|_| "Could not retain the original file; no recovery was applied")?;
            }
        }
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let write_result = (|| {
            let mut database = Database::open(&temporary)?;
            database.replace(&next)?;
            drop(database);
            fs::rename(&temporary, &self.path).map_err(|error| error.to_string())?;
            Ok::<_, String>(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result?;
        self.database = Database::open(&self.path)?;
        self.data = next;
        self.recovery = None;
        self.pending = None;
        // Primary is now valid. A failed backup is safe to retry on the next durable write.
        let _ = self.database.backup(&self.backup_path());
        self.snapshot()
    }

    pub fn new(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let process_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("finance-state.lock"))
            .map_err(|error| error.to_string())?;
        process_lock.try_lock().map_err(|error| {
            format!("Cannot lock financial data; another Brief instance may be open: {error}")
        })?;
        let path = directory.join("finance-state.sqlite3");
        let backup_path = directory.join("finance-state.backup.sqlite3");
        let existing = path.exists();
        let loaded = if existing {
            load_database(&path)
        } else if backup_path.exists() {
            Err("The primary finance database is missing".into())
        } else {
            legacy_state(&directory)
                .and_then(validate_state)
                .and_then(|data| {
                    let mut database = Database::open(&path)?;
                    database.replace(&data)?;
                    let _ = database.backup(&backup_path);
                    Ok((database, data))
                })
        };
        let (database, data, recovery) = match loaded {
            Ok((database, data)) => (database, data, None),
            Err(_) => {
                let backup = if backup_path.exists() {
                    load_database(&backup_path).ok().map(|(_, data)| data)
                } else {
                    read(&directory.join("finance-state.backup.json"))
                        .and_then(validate_state)
                        .ok()
                };
                let recovery = Recovery {
                    message: if backup.is_some() { "The saved database could not be read or requires a newer app version. Showing a retained valid snapshot, read-only, until you choose recovery." } else { "The saved database could not be read or requires a newer app version. No valid backup was found. Original files will be retained if you start a new snapshot." }.into(),
                    can_restore: backup.is_some(),
                };
                (
                    Database::open(Path::new(":memory:"))?,
                    backup.unwrap_or(empty_state()?),
                    Some(recovery),
                )
            }
        };
        Ok(Self {
            _process_lock: process_lock,
            path,
            database,
            data,
            pending: None,
            recovery,
        })
    }

    pub fn stage(
        &mut self,
        revision: u64,
        plaid_cache: PlaidCache,
        provider_data: ProviderDataCache,
        warnings: Vec<String>,
    ) -> Result<String, String> {
        self.ensure_writable()?;
        if revision != self.data.revision {
            return Err("Financial data changed during refresh; refresh again".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.pending = Some(PendingRefresh {
            id: id.clone(),
            started_at: chrono::Utc::now().to_rfc3339(),
            revision,
            plaid_cache,
            provider_data,
            warnings,
            expires: Instant::now() + Duration::from_secs(120),
        });
        Ok(id)
    }

    pub fn commit(&mut self, id: &str, mut snapshot: Value) -> Result<Value, String> {
        self.ensure_writable()?;
        let pending = self
            .pending
            .as_ref()
            .filter(|pending| {
                pending.id == id
                    && pending.revision == self.data.revision
                    && pending.expires > Instant::now()
            })
            .ok_or("Refresh expired or was superseded; refresh again")?;
        validate_snapshot(&snapshot)?;
        let revision = self
            .data
            .revision
            .checked_add(1)
            .ok_or("Snapshot revision overflow")?;
        snapshot["revision"] = revision.into();
        let mut warnings = pending.warnings.clone();
        warnings.extend(
            snapshot
                .get("syncWarnings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
        warnings.sort();
        warnings.dedup();
        snapshot["syncWarnings"] =
            serde_json::to_value(&warnings).map_err(|error| error.to_string())?;
        snapshot["providerStatus"] = serde_json::to_value(&pending.provider_data.sync_status)
            .map_err(|error| error.to_string())?;
        let next = FinanceState {
            schema_version: 1,
            revision,
            snapshot: snapshot.clone(),
            plaid_cache: pending.plaid_cache.clone(),
            provider_data: pending.provider_data.clone(),
            annotations: self.data.annotations.clone(),
            account_links: self.data.account_links.clone(),
        };
        let sync_run_warnings = (!warnings.is_empty())
            .then(|| vec![format!("{} warning(s)", warnings.len())])
            .unwrap_or_default();
        self.database.backup(&self.backup_path())?;
        self.database.replace_with_sync_run(
            &next,
            &pending.id,
            &pending.started_at,
            &chrono::Utc::now().to_rfc3339(),
            &sync_run_warnings,
        )?;
        self.data = next;
        self.pending = None;
        self.snapshot()
    }

    pub fn fail_pending(&mut self, id: &str, error_code: &str) {
        let Some(pending) = self.pending.as_ref().filter(|pending| pending.id == id) else {
            return;
        };
        let _ = self.database.record_sync_run(
            id,
            &pending.started_at,
            &chrono::Utc::now().to_rfc3339(),
            "failed",
            &(!pending.warnings.is_empty())
                .then(|| vec![format!("{} warning(s)", pending.warnings.len())])
                .unwrap_or_default(),
            Some(error_code),
        );
        self.pending = None;
    }

    pub fn save_annotations(
        &mut self,
        changes: BTreeMap<String, Annotation>,
        importing: bool,
    ) -> Result<BTreeMap<String, Annotation>, String> {
        if self.recovery.is_some() && importing {
            return Ok(self.data.annotations.clone());
        }
        self.ensure_writable()?;
        let mut next = self.data.clone();
        for (id, change) in changes {
            if id.is_empty()
                || id.len() > 200
                || change
                    .category
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty() || value.len() > 100)
            {
                return Err("Invalid transaction annotation".into());
            }
            let entry = next.annotations.entry(id).or_default();
            if change.category.is_some() && (!importing || entry.category.is_none()) {
                entry.category = change.category;
            }
            if change.reviewed.is_some() && (!importing || entry.reviewed.is_none()) {
                entry.reviewed = change.reviewed;
            }
            if change.benefit_confirmed.is_some()
                && (!importing || entry.benefit_confirmed.is_none())
            {
                entry.benefit_confirmed = change.benefit_confirmed;
            }
        }
        if next.annotations != self.data.annotations {
            self.database.backup(&self.backup_path())?;
            self.database.replace_annotations(&next.annotations)?;
            self.data = next;
        }
        Ok(self.data.annotations.clone())
    }

    pub fn save_account_link(
        &mut self,
        plaid_account_id: String,
        snaptrade_account_id: Option<String>,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        if !plaid_account_id.starts_with("plaid:")
            || plaid_account_id.len() > 220
            || snaptrade_account_id
                .as_ref()
                .is_some_and(|id| !id.starts_with("snaptrade:") || id.len() > 220)
        {
            return Err("Invalid account link".into());
        }
        if let Some(target) = snaptrade_account_id.as_deref() {
            if self
                .data
                .account_links
                .iter()
                .any(|(plaid, snaptrade)| plaid != &plaid_account_id && snaptrade == target)
            {
                return Err("That SnapTrade account is already linked".into());
            }
            let confirmed_candidate = self.data.snapshot["possibleDuplicateAccounts"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|candidate| {
                    candidate["plaidAccountId"].as_str() == Some(&plaid_account_id)
                        && candidate["snaptradeAccountId"].as_str() == Some(target)
                });
            if !confirmed_candidate {
                return Err("That account pair is not a current duplicate candidate".into());
            }
        } else if !self.data.account_links.contains_key(&plaid_account_id) {
            return Err("That account link does not exist".into());
        }
        let mut next = self.data.clone();
        if let Some(snaptrade_account_id) = snaptrade_account_id {
            next.account_links
                .insert(plaid_account_id, snaptrade_account_id);
        } else {
            next.account_links.remove(&plaid_account_id);
        }
        if next.account_links != self.data.account_links {
            self.database.backup(&self.backup_path())?;
            self.database.replace_account_links(&next.account_links)?;
            self.data = next;
            self.pending = None;
        }
        Ok(())
    }
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Invalid local data in {}: {error}", path.display()))
}

fn read_or_default<T: Default + DeserializeOwned>(path: &Path) -> Result<T, String> {
    if path.exists() {
        read(path)
    } else {
        Ok(T::default())
    }
}

#[cfg(test)]
fn atomic_write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| error.to_string())?;
    // Rename is the commit point: a directory flush failure must not report an uncommitted write.
    if let Some(parent) = path.parent() {
        let _ = fs::File::open(parent).and_then(|directory| directory.sync_all());
    }
    Ok(())
}

// Validate the persisted IPC contract independently of the renderer's Zod validation.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    #[serde(default)]
    calculation_version: Option<u32>,
    updated_at: String,
    net_worth: f64,
    #[serde(default)]
    net_worth_incomplete: bool,
    accounts: Vec<Account>,
    holdings: Vec<Holding>,
    #[serde(default)]
    trades: Vec<Trade>,
    transactions: Vec<Transaction>,
    #[serde(default)]
    account_movements: Vec<AccountMovement>,
    spending: Spending,
    net_worth_history: Vec<Point>,
    #[serde(default)]
    net_worth_history_estimated: bool,
    #[serde(default)]
    benchmark_history: Vec<Point>,
    #[serde(default)]
    brokerage_performance: Vec<Performance>,
    #[serde(default)]
    observed_net_worth_history: Vec<Point>,
    #[serde(default)]
    possible_duplicate_accounts: Vec<PossibleDuplicate>,
    last_change: Option<Change>,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    id: String,
    name: String,
    institution: String,
    r#type: String,
    value: Option<f64>,
    #[serde(default)]
    known_cost_basis: Option<f64>,
    #[serde(default)]
    known_unrealized_gain: Option<f64>,
    #[serde(default)]
    known_unrealized_gain_pct: Option<f64>,
    #[serde(default)]
    investment_income_ytd: Option<f64>,
    #[serde(default)]
    sale_proceeds_ytd: Option<f64>,
    #[serde(default)]
    sales_ytd: Option<u64>,
    #[serde(default)]
    estimated_realized_gain_ytd: Option<f64>,
    #[serde(default)]
    realized_gain_coverage: Option<String>,
    #[serde(default)]
    cost_basis_coverage: Option<String>,
    #[serde(default)]
    balance_as_of: Option<String>,
    #[serde(default)]
    balance_fetched_at: Option<String>,
    #[serde(default)]
    positions_as_of: Option<String>,
    #[serde(default)]
    activity_as_of: Option<String>,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Holding {
    ticker: String,
    name: String,
    account_id: String,
    shares: Option<f64>,
    price: Option<f64>,
    value: Option<f64>,
    cost_basis: Option<f64>,
    #[serde(default)]
    unrealized_gain: Option<f64>,
    daily_change_pct: Option<f64>,
    #[serde(default)]
    weekly_change_pct: Option<f64>,
    #[serde(default)]
    weekly_reference_price: Option<f64>,
    #[serde(default)]
    weekly_reference_date: Option<String>,
    total_change_pct: Option<f64>,
    #[serde(default)]
    market_as_of: Option<String>,
    color: String,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transaction {
    id: String,
    merchant: String,
    category: String,
    date: String,
    #[serde(default)]
    occurred_on: Option<String>,
    #[serde(default)]
    posted_on: Option<String>,
    amount: f64,
    account: String,
    account_id: Option<String>,
    pending: bool,
    description: Option<String>,
    logo_url: Option<String>,
    website: Option<String>,
    logo_name: Option<String>,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Trade {
    id: String,
    r#type: String,
    date: String,
    amount: f64,
    account: String,
    account_id: String,
    ticker: Option<String>,
    description: Option<String>,
    units: Option<f64>,
    price: Option<f64>,
    #[serde(default)]
    realized_cost_basis: Option<f64>,
    #[serde(default)]
    estimated_realized_gain: Option<f64>,
    #[serde(default)]
    estimated_realized_gain_pct: Option<f64>,
    #[serde(default)]
    realized_gain_method: Option<String>,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountMovement {
    id: String,
    observed_at: String,
    account_id: String,
    name: String,
    change: f64,
}
#[allow(dead_code)]
#[derive(Deserialize)]
struct Point {
    date: String,
    value: f64,
}
#[allow(dead_code)]
#[derive(Deserialize)]
struct Allocation {
    name: String,
    value: f64,
    percent: f64,
    color: String,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spending {
    month_total: f64,
    categories: Vec<Allocation>,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Performance {
    account_id: String,
    name: String,
    institution: String,
    current_value: f64,
    #[serde(default)]
    history_start: Option<String>,
    #[serde(default)]
    performance_method: Option<String>,
    #[serde(default)]
    history_source: Option<String>,
    points: Vec<PerformancePoint>,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PerformancePoint {
    date: String,
    value: f64,
    net_deposits: Option<f64>,
    sp500: Option<f64>,
    #[serde(default)]
    market_change: Option<f64>,
    #[serde(default)]
    market_change_pct: Option<f64>,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Change {
    observed_at: String,
    previous_updated_at: String,
    previous_net_worth: f64,
    net_worth_change: f64,
    account_changes: Vec<AccountChange>,
    new_transaction_ids: Vec<String>,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountChange {
    account_id: String,
    name: String,
    change: f64,
}
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PossibleDuplicate {
    plaid_account_id: String,
    snaptrade_account_id: String,
    description: String,
}

fn validate_snapshot(value: &Value) -> Result<(), String> {
    let snapshot: Snapshot = serde_json::from_value(value.clone())
        .map_err(|error| format!("Invalid financial snapshot: {error}"))?;
    chrono::DateTime::parse_from_rfc3339(&snapshot.updated_at)
        .map_err(|_| "Invalid snapshot timestamp")?;
    let ids = snapshot
        .accounts
        .iter()
        .map(|account| account.id.as_str())
        .collect::<BTreeSet<_>>();
    let total: f64 = snapshot
        .accounts
        .iter()
        .filter(|account| account.id != "all")
        .filter_map(|account| account.value)
        .sum();
    let all_accounts = snapshot
        .accounts
        .iter()
        .filter(|account| account.id == "all")
        .count();
    if all_accounts != 1
        || ids.len() != snapshot.accounts.len()
        || ids.contains("")
        || !total.is_finite()
        || (total - snapshot.net_worth).abs() > 0.011
        || snapshot.accounts.iter().any(|account| {
            account.id == "all"
                && account
                    .value
                    .is_none_or(|value| (value - total).abs() > 0.011)
        })
        || snapshot.net_worth_incomplete
            != snapshot
                .accounts
                .iter()
                .any(|account| account.id != "all" && account.value.is_none())
        || snapshot.holdings.iter().any(|holding| {
            holding.account_id == "all"
                || !ids.contains(holding.account_id.as_str())
                || [
                    holding.shares,
                    holding.price,
                    holding.value,
                    holding.cost_basis,
                    holding.unrealized_gain,
                    holding.daily_change_pct,
                    holding.weekly_change_pct,
                    holding.weekly_reference_price,
                    holding.total_change_pct,
                ]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
                || holding
                    .market_as_of
                    .as_deref()
                    .is_some_and(|timestamp| !valid_source_time(timestamp))
                || holding
                    .weekly_reference_date
                    .as_deref()
                    .is_some_and(|date| !valid_date(date))
                || holding.weekly_reference_price.is_some()
                    != holding.weekly_reference_date.is_some()
                || holding.weekly_change_pct.is_some() != holding.weekly_reference_price.is_some()
        })
        || snapshot.transactions.iter().any(|transaction| {
            transaction
                .account_id
                .as_ref()
                .is_some_and(|id| !ids.contains(id.as_str()))
        })
        || snapshot.trades.iter().any(|trade| {
            !ids.contains(trade.account_id.as_str())
                || !trade.amount.is_finite()
                || [
                    trade.realized_cost_basis,
                    trade.estimated_realized_gain,
                    trade.estimated_realized_gain_pct,
                ]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
                || trade
                    .realized_gain_method
                    .as_deref()
                    .is_some_and(|method| method != "estimated-fifo")
        })
        || snapshot.accounts.iter().any(|account| {
            account.value.is_some_and(|value| !value.is_finite())
                || [
                    account.known_cost_basis,
                    account.known_unrealized_gain,
                    account.known_unrealized_gain_pct,
                    account.investment_income_ytd,
                    account.sale_proceeds_ytd,
                    account.estimated_realized_gain_ytd,
                ]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
                || [
                    account.cost_basis_coverage.as_deref(),
                    account.realized_gain_coverage.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|coverage| !["complete", "partial", "unavailable"].contains(&coverage))
                || [
                    account.balance_as_of.as_deref(),
                    account.positions_as_of.as_deref(),
                    account.activity_as_of.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|timestamp| !valid_source_time(timestamp))
                || account
                    .balance_fetched_at
                    .as_deref()
                    .is_some_and(|timestamp| {
                        chrono::DateTime::parse_from_rfc3339(timestamp).is_err()
                    })
        })
    {
        return Err("Snapshot accounts, holdings, and net worth do not reconcile".into());
    }
    let transaction_ids = snapshot
        .transactions
        .iter()
        .map(|transaction| &transaction.id)
        .collect::<BTreeSet<_>>();
    if transaction_ids.len() != snapshot.transactions.len() {
        return Err("Duplicate transaction IDs".into());
    }
    if snapshot.transactions.iter().any(|transaction| {
        !transaction.amount.is_finite()
            || !valid_date(&transaction.date)
            || transaction
                .occurred_on
                .as_deref()
                .is_some_and(|date| !valid_date(date))
            || transaction
                .posted_on
                .as_deref()
                .is_some_and(|date| !valid_date(date))
    }) {
        return Err("Invalid transaction amounts or dates".into());
    }
    let trade_ids = snapshot
        .trades
        .iter()
        .map(|trade| &trade.id)
        .collect::<BTreeSet<_>>();
    if trade_ids.len() != snapshot.trades.len() {
        return Err("Duplicate trade IDs".into());
    }
    if snapshot.trades.iter().any(|trade| {
        !valid_date(&trade.date)
            || [Some(trade.amount), trade.units, trade.price]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
    }) {
        return Err("Invalid trade amounts or dates".into());
    }
    let movement_ids = snapshot
        .account_movements
        .iter()
        .map(|movement| &movement.id)
        .collect::<BTreeSet<_>>();
    if snapshot.account_movements.len() > 2_000
        || movement_ids.len() != snapshot.account_movements.len()
        || snapshot.account_movements.iter().any(|movement| {
            movement.id.is_empty()
                || movement.account_id.is_empty()
                || chrono::DateTime::parse_from_rfc3339(&movement.observed_at).is_err()
        })
    {
        return Err("Invalid account movement history".into());
    }
    if !valid_points(&snapshot.net_worth_history, true)
        || !valid_points(&snapshot.observed_net_worth_history, false)
        || !valid_points(&snapshot.benchmark_history, false)
    {
        return Err("Snapshot history is not ordered, unique, and finite".into());
    }
    let spending_total: f64 = snapshot
        .spending
        .categories
        .iter()
        .map(|category| category.value)
        .sum();
    let spending_percent: f64 = snapshot
        .spending
        .categories
        .iter()
        .map(|category| category.percent)
        .sum();
    let spending_names = snapshot
        .spending
        .categories
        .iter()
        .map(|category| category.name.as_str())
        .collect::<BTreeSet<_>>();
    if !snapshot.spending.month_total.is_finite()
        || snapshot.spending.month_total < 0.0
        || snapshot.spending.categories.iter().any(|category| {
            !category.value.is_finite()
                || category.value < 0.0
                || !category.percent.is_finite()
                || category.percent < 0.0
        })
        || (spending_total - snapshot.spending.month_total).abs() > 0.011
        || spending_names.len() != snapshot.spending.categories.len()
        || (snapshot.spending.month_total > 0.0 && (spending_percent - 100.0).abs() > 0.02)
        || (snapshot.spending.month_total == 0.0 && spending_percent.abs() > 0.001)
    {
        return Err("Spending totals do not reconcile".into());
    }
    if snapshot.brokerage_performance.iter().any(|performance| {
        (performance.account_id != "total" && !ids.contains(performance.account_id.as_str()))
            || !performance.current_value.is_finite()
            || performance
                .history_start
                .as_deref()
                .is_some_and(|date| !valid_date(date))
            || performance
                .performance_method
                .as_deref()
                .is_some_and(|method| {
                    ![
                        "value-only",
                        "value-with-comparisons",
                        "time-weighted",
                        "modified-dietz",
                    ]
                    .contains(&method)
                })
            || performance.history_source.as_deref().is_some_and(|source| {
                !["reported", "provider-estimated", "estimated", "unavailable"].contains(&source)
            })
            || !valid_performance_points(
                &performance.points,
                performance.performance_method.as_deref(),
            )
    }) {
        return Err("Brokerage performance is invalid or references an unknown account".into());
    }
    let mut possible_duplicate_pairs = BTreeSet::new();
    if snapshot
        .possible_duplicate_accounts
        .iter()
        .any(|candidate| {
            candidate.description.trim().is_empty()
                || !candidate.plaid_account_id.starts_with("plaid:")
                || !candidate.snaptrade_account_id.starts_with("snaptrade:")
                || !ids.contains(candidate.plaid_account_id.as_str())
                || !ids.contains(candidate.snaptrade_account_id.as_str())
                || !possible_duplicate_pairs.insert((
                    candidate.plaid_account_id.as_str(),
                    candidate.snaptrade_account_id.as_str(),
                ))
        })
    {
        return Err("Invalid possible duplicate account candidates".into());
    }
    if snapshot
        .calculation_version
        .is_some_and(|version| version == 0)
    {
        return Err("Invalid financial calculation version".into());
    }
    if let Some(change) = &snapshot.last_change {
        let sum: f64 = change
            .account_changes
            .iter()
            .map(|account| account.change)
            .sum();
        if !change.net_worth_change.is_finite()
            || !change.previous_net_worth.is_finite()
            || (sum - change.net_worth_change).abs() > 0.011
            || chrono::DateTime::parse_from_rfc3339(&change.observed_at).is_err()
            || chrono::DateTime::parse_from_rfc3339(&change.previous_updated_at).is_err()
            || change
                .account_changes
                .iter()
                .any(|account| account.account_id.is_empty() || !account.change.is_finite())
            || change
                .new_transaction_ids
                .iter()
                .any(|id| !transaction_ids.contains(id))
        {
            return Err("Snapshot change does not reconcile".into());
        }
    }
    Ok(())
}

fn valid_date(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

fn valid_source_time(value: &str) -> bool {
    valid_date(value) || chrono::DateTime::parse_from_rfc3339(value).is_ok()
}

fn valid_points(points: &[Point], allow_legacy_empty: bool) -> bool {
    let mut previous = None;
    points.iter().all(|point| {
        if !point.value.is_finite()
            || (point.date == "—" && !(allow_legacy_empty && points.len() == 1))
        {
            return false;
        }
        if point.date != "—" && !valid_date(&point.date) {
            return false;
        }
        let ordered = previous.is_none_or(|date: &str| date < point.date.as_str());
        previous = Some(point.date.as_str());
        ordered
    })
}

fn valid_performance_points(points: &[PerformancePoint], method: Option<&str>) -> bool {
    let mut previous = None;
    points.iter().all(|point| {
        let ordered = previous.is_none_or(|date: &str| date < point.date.as_str());
        previous = Some(point.date.as_str());
        valid_date(&point.date)
            && ordered
            && point.value.is_finite()
            && point.net_deposits.is_none_or(f64::is_finite)
            && point.sp500.is_none_or(f64::is_finite)
            && point.market_change.is_none_or(f64::is_finite)
            && point.market_change_pct.is_none_or(f64::is_finite)
            && if method == Some("value-only") {
                point.net_deposits.is_none()
                    && point.sp500.is_none()
                    && point.market_change.is_none()
                    && point.market_change_pct.is_none()
            } else {
                point.net_deposits.is_some()
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::SnapTradeData;

    #[test]
    fn older_calculation_versions_rebuild_from_the_committed_cache() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        storage.data.snapshot["updatedAt"] = "2026-09-10T07:30:00Z".into();
        storage.data.snapshot["calculationVersion"] = 4.into();
        storage.data.snapshot["benchmarkHistory"] = serde_json::json!([
            { "date": "2026-09-08", "value": 100 },
            { "date": "2026-09-10", "value": 101 }
        ]);
        storage.data.provider_data.snaptrade = Some(SnapTradeData {
            accounts: vec![serde_json::json!({
                "id": "brokerage", "name": "Brokerage", "institution_name": "Broker",
                "balance": { "total": { "amount": 200, "currency": "USD" } }
            })],
            activities: BTreeMap::from([(
                "brokerage".into(),
                vec![serde_json::json!({
                    "id": "transfer", "type": "TRANSFER", "amount": 10,
                    "trade_date": "2026-09-09"
                })],
            )]),
            balance_history: BTreeMap::from([(
                "brokerage".into(),
                vec![serde_json::json!({
                    "date": "2026-09-08", "total_value": 190
                })],
            )]),
            history_complete: true,
            activity_complete: true,
            ..SnapTradeData::default()
        });

        assert!(storage.upgrade_projection().unwrap());
        assert_eq!(
            storage.data.snapshot["calculationVersion"],
            crate::financial_engine::CALCULATION_VERSION
        );
        let account = &storage.data.snapshot["brokeragePerformance"][1];
        assert_eq!(account["points"][1]["netDeposits"], 200.0);
        assert_eq!(account["points"][1]["sp500"], 201.9);
        drop(storage);
        assert_eq!(
            Storage::new(directory.clone()).unwrap().data.snapshot["calculationVersion"],
            crate::financial_engine::CALCULATION_VERSION
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupted_primary_opens_read_only_backup_and_restores_without_discarding_original() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let mut snapshot = storage.data.snapshot.clone();
        snapshot["updatedAt"] = "2026-09-01T12:00:00Z".into();
        let id = storage
            .stage(
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        storage.commit(&id, snapshot.clone()).unwrap();
        let id = storage
            .stage(
                1,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        storage.commit(&id, snapshot).unwrap();
        fs::write(&storage.path, b"synthetic corrupt state").unwrap();
        drop(storage);
        let mut storage = Storage::new(directory.clone()).unwrap();
        assert_eq!(storage.data.revision, 1);
        assert_eq!(storage.snapshot().unwrap()["recovery"]["canRestore"], true);
        assert!(storage
            .stage(
                1,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![]
            )
            .is_err());
        assert!(storage.save_annotations(BTreeMap::new(), true).is_ok());
        assert_eq!(fs::read(&storage.path).unwrap(), b"synthetic corrupt state");
        let restored = storage.recover(true).unwrap();
        assert!(restored.get("recovery").is_none());
        assert_eq!(load_database(&storage.path).unwrap().1.revision, 1);
        assert!(fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .any(
                |entry| entry.file_name().to_string_lossy().contains("retained-")
                    && fs::read(entry.path()).unwrap() == b"synthetic corrupt state"
            ));
        drop(storage);
        assert!(Storage::new(directory.clone()).unwrap().recovery.is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_without_a_valid_backup_requires_an_explicit_fresh_start() {
        let directory = directory();
        let storage = Storage::new(directory.clone()).unwrap();
        fs::write(&storage.path, b"corrupt primary").unwrap();
        fs::write(storage.backup_path(), b"corrupt backup").unwrap();
        drop(storage);
        let mut storage = Storage::new(directory.clone()).unwrap();
        assert!(!storage.recovery.as_ref().unwrap().can_restore);
        assert!(storage.recover(true).is_err());
        assert!(storage.recover(false).is_ok());
        assert_eq!(storage.data.revision, 0);
        assert_eq!(
            fs::read_dir(&directory)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains("retained-"))
                .count(),
            2
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_missing_primary_does_not_overwrite_a_valid_retained_snapshot() {
        let directory = directory();
        let storage = Storage::new(directory.clone()).unwrap();
        fs::remove_file(&storage.path).unwrap();
        drop(storage);
        let storage = Storage::new(directory.clone()).unwrap();
        assert!(storage.recovery.as_ref().unwrap().can_restore);
        assert!(!storage.path.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn nullable_values_require_an_explicit_incomplete_subtotal() {
        let mut snapshot = empty_state().unwrap().snapshot;
        snapshot["accounts"] = serde_json::json!([
            {"id": "all", "name": "All", "institution": "Brief", "type": "combined", "value": 100},
            {"id": "cash", "name": "Cash", "institution": "Bank", "type": "cash", "value": 100},
            {"id": "card", "name": "Card", "institution": "Bank", "type": "credit", "value": null}
        ]);
        snapshot["netWorth"] = 100.into();
        snapshot["holdings"] = serde_json::json!([{"ticker": "UNPRICED", "name": "Unpriced", "accountId": "cash", "shares": null, "price": null, "value": null, "costBasis": null, "dailyChangePct": null, "totalChangePct": null, "color": "#000000"}]);
        assert!(validate_snapshot(&snapshot).is_err());
        snapshot["netWorthIncomplete"] = true.into();
        assert!(validate_snapshot(&snapshot).is_ok());
    }

    #[test]
    fn confirmed_account_links_are_one_to_one() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        storage.data.snapshot["accounts"] = serde_json::json!([
            {"id": "all", "name": "All", "institution": "Brief", "type": "combined", "value": 0},
            {"id": "plaid:one", "name": "One", "institution": "Broker", "type": "brokerage", "value": 0},
            {"id": "plaid:two", "name": "Two", "institution": "Broker", "type": "brokerage", "value": 0},
            {"id": "snaptrade:same", "name": "Same", "institution": "Broker", "type": "brokerage", "value": 0}
        ]);
        storage.data.snapshot["possibleDuplicateAccounts"] = serde_json::json!([
            {"plaidAccountId": "plaid:one", "snaptradeAccountId": "snaptrade:same", "description": "Candidate one"},
            {"plaidAccountId": "plaid:two", "snaptradeAccountId": "snaptrade:same", "description": "Candidate two"}
        ]);
        storage
            .save_account_link("plaid:one".into(), Some("snaptrade:same".into()))
            .unwrap();
        assert!(storage
            .save_account_link("plaid:two".into(), Some("snaptrade:same".into()))
            .is_err());
        assert_eq!(storage.data.account_links.len(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("brief-storage-{}", uuid::Uuid::new_v4()))
    }
    #[test]
    fn refresh_is_atomic_validated_and_single_use() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let original = storage.data.snapshot.clone();
        let cache: PlaidCache = serde_json::from_value(
            serde_json::json!({"items": {"item": {"cursor": "next", "transactions": []}}}),
        )
        .unwrap();
        let id = storage
            .stage(
                0,
                cache,
                ProviderDataCache::default(),
                vec!["Offline".into()],
            )
            .unwrap();
        assert_eq!(storage.database.load().unwrap().unwrap().revision, 0);
        assert!(storage
            .commit(&id, serde_json::json!({"netWorth": 999}))
            .is_err());
        let mut invalid = original.clone();
        invalid["netWorth"] = 999.into();
        assert!(storage.commit(&id, invalid).is_err());
        let result = storage.commit(&id, original.clone()).unwrap();
        assert_eq!(result["syncWarnings"][0], "Offline");
        assert_eq!(storage.database.load().unwrap().unwrap().revision, 1);
        assert_eq!(
            serde_json::to_value(&storage.database.load().unwrap().unwrap().plaid_cache).unwrap()
                ["items"]["item"]["cursor"],
            "next"
        );
        assert!(storage.commit(&id, original).is_err());
        assert!(storage
            .stage(
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![]
            )
            .is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn superseded_refresh_and_unknown_versions_are_rejected() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let old = storage
            .stage(
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        storage
            .stage(
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        assert!(storage.commit(&old, storage.data.snapshot.clone()).is_err());
        storage.data.schema_version = 2;
        storage.database.replace(&storage.data).unwrap();
        drop(storage);
        let recovery = Storage::new(directory.clone()).unwrap();
        assert!(recovery.recovery.is_some());
        assert!(recovery.ensure_writable().is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn migrates_without_removing_legacy_data() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let legacy = directory.join("finance-snapshot.json");
        let mut snapshot: Value =
            serde_json::from_str(include_str!("../../src/data/empty.json")).unwrap();
        snapshot["accounts"][0]["balanceAsOf"] = "2026-09-05".into();
        atomic_write(&legacy, &snapshot).unwrap();
        let storage = Storage::new(directory.clone()).unwrap();
        assert_eq!(storage.data.snapshot, snapshot);
        assert!(legacy.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_legacy_state_opens_recovery_without_creating_a_database() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let legacy = directory.join("finance-state.json");
        let mut state = empty_state().unwrap();
        state.snapshot["netWorth"] = 999.into();
        atomic_write(&legacy, &state).unwrap();

        let storage = Storage::new(directory.clone()).unwrap();

        assert!(storage.recovery.is_some());
        assert!(!storage.path.exists());
        assert!(legacy.exists());
        assert!(storage.ensure_writable().is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_write_preserves_disk_memory_and_pending_refresh() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let id = storage
            .stage(
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        let mut invalid = storage.data.snapshot.clone();
        invalid["possibleDuplicateAccounts"] = serde_json::json!([
            {"plaidAccountId": "same", "snaptradeAccountId": "one", "description": "one"},
            {"plaidAccountId": "same", "snaptradeAccountId": "one", "description": "duplicate"}
        ]);
        assert!(storage.commit(&id, invalid).is_err());
        assert_eq!(storage.data.revision, 0);
        assert_eq!(storage.database.load().unwrap().unwrap().revision, 0);
        storage.commit(&id, storage.data.snapshot.clone()).unwrap();
        assert_eq!(storage.data.revision, 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn annotations_survive_refresh_and_legacy_import_never_overwrites_edits() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let id = storage
            .stage(
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        let edits = BTreeMap::from([(
            "transaction".into(),
            Annotation {
                category: Some("Travel".into()),
                reviewed: Some(true),
                benefit_confirmed: Some(true),
            },
        )]);
        storage.save_annotations(edits, false).unwrap();
        let legacy = BTreeMap::from([(
            "transaction".into(),
            Annotation {
                category: Some("Other".into()),
                reviewed: Some(false),
                benefit_confirmed: Some(false),
            },
        )]);
        storage.save_annotations(legacy, true).unwrap();
        storage.commit(&id, storage.data.snapshot.clone()).unwrap();
        drop(storage);
        let restored = Storage::new(directory.clone()).unwrap();
        assert_eq!(
            restored.data.annotations["transaction"].category.as_deref(),
            Some("Travel")
        );
        assert_eq!(
            restored.data.annotations["transaction"].reviewed,
            Some(true)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn expired_refresh_does_not_commit() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let id = storage
            .stage(
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        storage.pending.as_mut().unwrap().expires = Instant::now() - Duration::from_secs(1);
        assert!(storage.commit(&id, storage.data.snapshot.clone()).is_err());
        assert_eq!(storage.data.revision, 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn only_one_app_instance_can_own_the_store() {
        let directory = directory();
        let storage = Storage::new(directory.clone()).unwrap();
        assert!(Storage::new(directory.clone()).is_err());
        drop(storage);
        assert!(Storage::new(directory.clone()).is_ok());
        fs::remove_dir_all(directory).unwrap();
    }
}
