//! Opt-in research through IPC. Every command that sends anything checks that research is on
//! and logs the query before it runs. Pages that come back are shown to the user and never
//! acted on; a rule joins settings only through the user's own settings save.
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::{
    evidence::programs::{programs, Program},
    research::{self, Draft, Entry, EntryKind, Gap, SourceCheck},
    AppState,
};

/// What the research sheet shows.
#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchState")]
pub(crate) struct ResearchState {
    enabled: bool,
    connected: bool,
    gaps: Vec<Gap>,
    log: Vec<Entry>,
    sources: Vec<SourceCheck>,
}

fn enabled(state: &State<'_, AppState>) -> Result<(), String> {
    if state.storage()?.settings().research {
        Ok(())
    } else {
        Err("Turn on research in Settings first".into())
    }
}

/// Logs an entry before it runs and tells the renderer, so the query is visible first.
fn begin(app: &AppHandle, state: &State<'_, AppState>, entry: &Entry) -> Result<(), String> {
    state.storage()?.begin_research(entry)?;
    let _ = app.emit("research-updated", ());
    Ok(())
}

fn finish(app: &AppHandle, state: &State<'_, AppState>, entry: &Entry) -> Result<(), String> {
    state.storage()?.finish_research(entry)?;
    let _ = app.emit("research-updated", ());
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_research(state: State<'_, AppState>) -> Result<ResearchState, String> {
    let connected = state.providers.integration_status()?.exa;
    let storage = state.storage()?;
    let view = storage.read_view()?;
    let settings = storage.settings();
    let gaps = view.insights.as_ref().map_or_else(Vec::new, |insights| {
        research::gaps(
            &view.snapshot.accounts,
            &insights.evidence.chapters,
            &programs(&settings.programs),
        )
    });
    let (log, sources) = storage.research()?;
    Ok(ResearchState {
        enabled: settings.research,
        connected,
        gaps,
        log,
        sources,
    })
}

/// Searches for a gap's terms with the query the sheet showed.
#[tauri::command]
pub(crate) async fn research_gap(
    app: AppHandle,
    state: State<'_, AppState>,
    gap_id: String,
) -> Result<Entry, String> {
    enabled(&state)?;
    let gap = get_research(state.clone())
        .await?
        .gaps
        .into_iter()
        .find(|gap| gap.id == gap_id)
        .ok_or("That gap is no longer open")?;
    let mut entry = Entry::begin(
        EntryKind::Search,
        gap.query,
        Some(gap.id),
        chrono::Utc::now(),
    );
    begin(&app, &state, &entry)?;
    let result = state.providers.exa_search(&entry.query).await;
    entry.finish(result, chrono::Utc::now());
    finish(&app, &state, &entry)?;
    Ok(entry)
}

/// Reads each rule's source again when a month has passed since the last read, or every
/// source when `all`. A page that differs from the last read flags its rule.
#[tauri::command]
pub(crate) async fn recheck_program_sources(
    app: AppHandle,
    state: State<'_, AppState>,
    all: bool,
) -> Result<Vec<Entry>, String> {
    enabled(&state)?;
    let now = chrono::Utc::now();
    let due = {
        let storage = state.storage()?;
        let mut due = Vec::<(String, String)>::new();
        for program in programs(&storage.settings().programs) {
            if due.iter().any(|(url, _)| *url == program.source.url) {
                continue;
            }
            if all || research::due(storage.source_check(&program.source.url)?.as_ref(), now) {
                due.push((program.source.url, program.id));
            }
        }
        due
    };
    let mut entries = Vec::new();
    for (url, program) in due {
        let mut entry = Entry::begin(EntryKind::Recheck, url.clone(), Some(program), now);
        begin(&app, &state, &entry)?;
        let result = state
            .providers
            .exa_contents(std::slice::from_ref(&url))
            .await
            .and_then(|pages| {
                pages
                    .into_iter()
                    .find(|page| !page.text.is_empty())
                    .ok_or_else(|| "Exa returned no text for this page".to_string())
            });
        if let Ok(page) = &result {
            let storage = state.storage()?;
            let check =
                research::checked(storage.source_check(&url)?.as_ref(), &url, &page.text, now);
            storage.save_source_check(&check)?;
        }
        // The log keeps that the page was read and how it began, not all its text again.
        entry.finish(
            result.map(|mut page| {
                page.text = page.text.chars().take(400).collect();
                vec![page]
            }),
            chrono::Utc::now(),
        );
        finish(&app, &state, &entry)?;
        entries.push(entry);
    }
    Ok(entries)
}

/// Marks a changed source reviewed.
#[tauri::command]
pub(crate) async fn acknowledge_program_source(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    let storage = state.storage()?;
    let mut check = storage
        .source_check(&url)?
        .ok_or("That source hasn't been read")?;
    check.changed_at = None;
    storage.save_source_check(&check)?;
    let _ = app.emit("research-updated", ());
    Ok(())
}

/// The rule a draft describes, checked as a built-in rule is, for the user to approve. Nothing
/// is saved here.
#[tauri::command]
pub(crate) fn draft_program(draft: Draft) -> Result<Program, String> {
    research::draft(&draft, chrono::Local::now().date_naive())
}
