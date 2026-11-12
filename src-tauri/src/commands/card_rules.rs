//! Card rules through IPC. The catalog is downloaded whole, at most daily, so no request names
//! a card. After each refresh and at launch, clear catalog matches are applied in the
//! background; a terms page is read only with research on and logged like any research query.
use std::collections::BTreeMap;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use super::research::{begin, enabled, finish};
use crate::{
    card_drafts::{
        self, catalog, history,
        programs::{self, ProgramPrefill},
        terms, CardDraft, CardRulesChange, CatalogCard,
    },
    finance_contract::{Account, Transaction},
    insights::benefits::{CardRules, RuleOrigin},
    providers::{
        card_catalog::CatalogCardEntry,
        exa::Page,
        reading::{self, Task},
    },
    research::{self, Entry, EntryKind},
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

/// Whether a terms page may be read now, or why not.
fn can_read_terms(state: &State<'_, AppState>) -> Result<(), String> {
    enabled(state)?;
    if state.providers.integration_status()?.exa {
        Ok(())
    } else {
        Err("Connect Exa in Settings to check the issuer's terms".into())
    }
}

/// Reads a card's terms page and logs it; the log keeps how the page began.
async fn read_terms_page(
    app: &AppHandle,
    state: &State<'_, AppState>,
    card: &CatalogCardEntry,
) -> Result<Page, String> {
    let now = chrono::Utc::now();
    let url = match card.url.clone().filter(|url| url.starts_with("https://")) {
        Some(url) => url,
        None => {
            let query = format!(
                "{} card benefits, annual fee, and statement credit terms",
                card.name
            );
            let mut entry =
                Entry::begin(EntryKind::Search, query.clone(), Some(card.id.clone()), now);
            begin(app, state, &entry)?;
            let result = state.providers.exa_search(&query).await;
            let url = result
                .as_ref()
                .ok()
                .and_then(|pages| pages.first())
                .map(|page| page.url.clone());
            entry.finish(result, chrono::Utc::now());
            finish(app, state, &entry)?;
            url.ok_or("No page with the card's terms was found")?
        }
    };
    let mut entry = Entry::begin(EntryKind::Recheck, url.clone(), Some(card.id.clone()), now);
    begin(app, state, &entry)?;
    let result = state.providers.exa_terms_page(&url).await;
    if let Ok(page) = &result {
        let storage = state.storage()?;
        let check = research::checked(storage.source_check(&url)?.as_ref(), &url, &page.text, now);
        storage.save_source_check(&check)?;
    }
    entry.finish(
        result
            .as_ref()
            .map(|page| {
                vec![Page {
                    text: page.text.chars().take(400).collect(),
                    ..page.clone()
                }]
            })
            .map_err(Clone::clone),
        chrono::Utc::now(),
    );
    finish(app, state, &entry)?;
    result
}

/// Sentence picks from a model for the benefits the rule couldn't find evidence for. A model
/// that isn't available, or an answer that doesn't read, leaves the rule's result as it is.
async fn model_picks(
    state: &State<'_, AppState>,
    draft: &CardDraft,
    text: &str,
) -> BTreeMap<String, Vec<usize>> {
    let local = state
        .storage()
        .ok()
        .and_then(|storage| storage.settings().local_model.clone());
    let sentences = reading::sentences(text);
    let offered = terms::candidates(&sentences);
    let shown = offered
        .iter()
        .map(|index| sentences[*index].clone())
        .collect::<Vec<_>>();
    let mut picks = BTreeMap::new();
    for rule in &draft.rules.benefits {
        if terms::evidence(rule, &sentences, 0..sentences.len()).is_some() || shown.is_empty() {
            continue;
        }
        let input = reading::numbered(
            &format!("Benefit: {}", rule.name),
            &shown,
            reading::APPLE_CHARS,
        );
        let Ok((reply, _)) = state
            .providers
            .read_text(Task::Benefit, input, reading::local_model(local.as_deref()))
            .await
        else {
            continue;
        };
        let listed = reading::listed(&reply, &shown);
        let indexes = listed
            .iter()
            .filter_map(|sentence| sentences.iter().position(|candidate| candidate == sentence))
            .collect::<Vec<_>>();
        picks.insert(rule.id.clone(), terms::verified(&sentences, &indexes));
    }
    picks
}

async fn drafted(
    app: &AppHandle,
    state: &State<'_, AppState>,
    card: &CatalogCardEntry,
    account: Option<&Account>,
    transactions: &[Transaction],
    read_terms: bool,
) -> Result<CardDraft, String> {
    let today = chrono::Local::now().date_naive();
    let mut draft = catalog::draft(card, account, today);
    history::observe(&mut draft, transactions);
    if read_terms {
        match can_read_terms(state) {
            Err(why) => draft.warnings.push(why),
            Ok(()) => match read_terms_page(app, state, card).await {
                Err(error) => draft
                    .warnings
                    .push(format!("The terms couldn't be read: {error}")),
                Ok(page) => {
                    let picks = model_picks(state, &draft, &page.text).await;
                    terms::apply(&mut draft, &page.url, &page.text, &picks, today);
                }
            },
        }
    }
    Ok(draft)
}

/// Rules drafted for a credit card from its catalog entry (the best match unless one is
/// given), its posted credits, and, when asked and research allows, the issuer's terms.
#[tauri::command]
pub(crate) async fn draft_card_rules(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    catalog_id: Option<String>,
    read_terms: bool,
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
    let mut draft = drafted(&app, &state, card, Some(account), &transactions, read_terms).await?;
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

/// Saved rules whose catalog entry changed, or whose terms page changed when it was due for a
/// read, each with a fresh draft and its differences from what was approved.
#[tauri::command]
pub(crate) async fn recheck_card_rules(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<CardRulesChange>, String> {
    let saved_rules = state.storage()?.settings().card_rules.clone();
    if saved_rules.is_empty() {
        return Ok(Vec::new());
    }
    let (accounts, transactions) = saved(&state)?;
    let catalog = state.providers.card_catalog(true).await?;
    let can_read = can_read_terms(&state).is_ok();
    let now = chrono::Utc::now();
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
        let mut reasons = Vec::new();
        if card.hash != reference.hash {
            reasons.push("the catalog entry");
        }
        let due = match &rules.terms_url {
            Some(url) if can_read => {
                research::due(state.storage()?.source_check(url)?.as_ref(), now)
            }
            _ => false,
        };
        let account = account_for(&rules, &accounts, &state);
        let mut draft = drafted(&app, &state, card, account, &transactions, due).await?;
        if due && draft.rules.terms_hash.is_some() && draft.rules.terms_hash != rules.terms_hash {
            reasons.push("the terms page");
        }
        if reasons.is_empty() {
            continue;
        }
        draft.candidates = Vec::new();
        changes.push(CardRulesChange {
            product: rules.product.clone(),
            name: rules.name.clone(),
            reason: match reasons.as_slice() {
                [_, _] => "The catalog entry and the issuer's terms page changed since you saved these rules.",
                ["the terms page"] => "The issuer's terms page changed since you saved these rules.",
                _ => "The catalog entry changed since you saved these rules.",
            }
            .into(),
            changes: card_drafts::diff(&rules, &draft.rules),
            draft,
        });
    }
    Ok(changes)
}

/// A program draft for a research gap, prefilled from a page the user picked.
#[tauri::command]
pub(crate) async fn prefill_program_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    gap_id: String,
    url: String,
) -> Result<ProgramPrefill, String> {
    can_read_terms(&state)?;
    if !url.starts_with("https://") {
        return Err("Pick a page with an https address".into());
    }
    let gap = super::research::get_research(state.clone())
        .await?
        .gaps
        .into_iter()
        .find(|gap| gap.id == gap_id)
        .ok_or("That gap is no longer open")?;
    let now = chrono::Utc::now();
    let mut entry = Entry::begin(EntryKind::Recheck, url.clone(), Some(gap.id.clone()), now);
    begin(&app, &state, &entry)?;
    let result = state.providers.exa_terms_page(&url).await;
    entry.finish(
        result
            .as_ref()
            .map(|page| {
                vec![Page {
                    text: page.text.chars().take(400).collect(),
                    ..page.clone()
                }]
            })
            .map_err(Clone::clone),
        chrono::Utc::now(),
    );
    finish(&app, &state, &entry)?;
    Ok(programs::prefill(
        &gap,
        &result?,
        chrono::Local::now().date_naive(),
    ))
}
