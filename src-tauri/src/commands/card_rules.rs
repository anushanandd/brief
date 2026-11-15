//! Card rules through IPC. The catalog is downloaded whole, at most daily, so no request names
//! a card. After each refresh and at launch, clear catalog matches are applied in the
//! background.
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    card_drafts::{self, catalog, history, CardDraft, CardRulesChange, CatalogCard},
    finance_contract::{Account, Transaction},
    insights::benefits::{CardRules, RuleOrigin},
    providers::card_catalog::CatalogCardEntry,
    AppState,
};

#[tauri::command]
pub(crate) async fn search_card_catalog(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<CatalogCard>, String> {
    let catalog = state.providers.card_catalog(false).await?;
    Ok(catalog::search(&catalog, &query))
}

/// Every catalog brand, for the renderer to warm logos with; none while the catalog is off.
#[tauri::command]
pub(crate) async fn catalog_brands(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    if state.storage()?.settings().card_catalog_off {
        return Ok(Vec::new());
    }
    let catalog = state.providers.card_catalog(false).await?;
    Ok(catalog::brands(&catalog))
}

/// The accounts and transactions a draft reads, copied out of storage before any network call.
fn saved(state: &State<'_, AppState>) -> Result<(Vec<Account>, Vec<Transaction>), String> {
    let view = state.storage()?.read_view()?;
    Ok((
        view.snapshot.accounts.clone(),
        view.snapshot.transactions.clone(),
    ))
}

fn drafted(
    card: &CatalogCardEntry,
    account: Option<&Account>,
    transactions: &[Transaction],
) -> CardDraft {
    let mut draft = catalog::draft(card, account, chrono::Local::now().date_naive());
    history::observe(&mut draft, transactions);
    draft
}

/// Rules drafted for a credit card from its catalog entry (the best match unless one is
/// given) and its posted credits.
#[tauri::command]
pub(crate) async fn draft_card_rules(
    state: State<'_, AppState>,
    account_id: String,
    catalog_id: Option<String>,
) -> Result<CardDraft, String> {
    let (accounts, transactions) = saved(&state)?;
    let account = accounts
        .iter()
        .find(|account| account.id == account_id && account.r#type == "credit")
        .ok_or("That credit card isn't in Brief")?;
    let catalog = state.providers.card_catalog(false).await?;
    let candidates = catalog::candidates(&catalog, account);
    let chosen = match &catalog_id {
        Some(id) => Some(catalog.card(id).ok_or("That card isn't in the catalog")?),
        None => candidates.first().copied(),
    };
    let Some(card) = chosen else {
        return Err("No catalog card matches this account; search the catalog for it".into());
    };
    let mut draft = drafted(card, Some(account), &transactions);
    draft.candidates = candidates
        .into_iter()
        .filter(|candidate| candidate.id != card.id)
        .map(catalog::summary)
        .collect();
    Ok(draft)
}

/// Applies the catalog to saved settings: clear matches saved, close calls listed, rules Brief
/// applied kept current. Runs off the main thread after the refresh lock; failures leave
/// settings as they were.
pub fn apply_in_background(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Ok(wanted) = state.storage().map(|storage| {
            !storage.is_recovering()
                && card_drafts::apply::wanted(
                    &storage.data.settings,
                    &storage.data.snapshot.accounts,
                )
        }) else {
            return;
        };
        if !wanted {
            return;
        }
        let Ok(catalog) = state.providers.card_catalog(false).await else {
            return;
        };
        let _refresh = state.refresh.lock().await;
        let saved = state.storage().and_then(|mut storage| {
            let view = storage.read_view()?;
            let settings = storage.settings().clone();
            let next = card_drafts::apply::apply(
                &settings,
                &view.snapshot.accounts,
                &view.snapshot.transactions,
                &catalog,
                chrono::Local::now().date_naive(),
            );
            if next == settings {
                return Ok(false);
            }
            storage.save_settings(next, false).map(|_| true)
        });
        if saved == Ok(true) {
            let _ = app.emit("finance-snapshot-updated", ());
        }
    });
}

/// Applies a catalog card's rules to a credit card, as the answer to "Which card is this?".
#[tauri::command]
pub(crate) async fn choose_catalog_card(
    state: State<'_, AppState>,
    account_id: String,
    catalog_id: String,
) -> Result<Value, String> {
    let catalog = state.providers.card_catalog(false).await?;
    let card = catalog
        .card(&catalog_id)
        .ok_or("That card isn't in the catalog")?;
    let _refresh = state.refresh.lock().await;
    let mut storage = state.storage()?;
    let view = storage.read_view()?;
    let account = view
        .snapshot
        .accounts
        .iter()
        .find(|account| account.id == account_id && account.r#type == "credit")
        .ok_or("That credit card isn't in Brief")?;
    let mut settings = storage.settings().clone();
    let today = chrono::Local::now().date_naive();
    if !card_drafts::apply::adopt(
        &mut settings,
        account,
        card,
        &view.snapshot.transactions,
        today,
    ) {
        return Err("The catalog lists no credits Brief can track for this card".into());
    }
    storage.save_settings(settings, false)
}

/// The credit card a product's rules are used for: the one assigned it, else the spending card.
fn account_for<'a>(
    rules: &CardRules,
    accounts: &'a [Account],
    state: &State<'_, AppState>,
) -> Option<&'a Account> {
    let storage = state.storage().ok()?;
    let settings = storage.settings();
    let id = settings
        .benefit_cards
        .iter()
        .find(|(_, product)| **product == rules.product)
        .map(|(id, _)| id.clone())
        .or_else(|| settings.spending_account_id.clone())?;
    accounts.iter().find(|account| account.id == id)
}

/// Saved rules whose catalog entry changed, each with a fresh draft and its differences from
/// what was approved.
#[tauri::command]
pub(crate) async fn recheck_card_rules(
    state: State<'_, AppState>,
) -> Result<Vec<CardRulesChange>, String> {
    let saved_rules = state.storage()?.settings().card_rules.clone();
    if saved_rules.is_empty() {
        return Ok(Vec::new());
    }
    let (accounts, transactions) = saved(&state)?;
    let catalog = state.providers.card_catalog(true).await?;
    let mut changes = Vec::new();
    // Rules Brief applied follow the catalog by themselves; only the user's are rechecked.
    for rules in saved_rules
        .into_iter()
        .filter(|rules| rules.origin == RuleOrigin::User)
    {
        let Some(reference) = &rules.catalog else {
            continue;
        };
        let Some(card) = catalog.card(&reference.id) else {
            continue;
        };
        if card.hash == reference.hash {
            continue;
        }
        let account = account_for(&rules, &accounts, &state);
        let mut draft = drafted(card, account, &transactions);
        draft.candidates = Vec::new();
        changes.push(CardRulesChange {
            product: rules.product.clone(),
            name: rules.name.clone(),
            reason: "The catalog entry changed since you saved these rules.".into(),
            changes: card_drafts::diff(&rules, &draft.rules),
            draft,
        });
    }
    Ok(changes)
}
