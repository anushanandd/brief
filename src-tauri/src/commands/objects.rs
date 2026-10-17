//! The object graph through IPC: one search over every object, and one object's raw view.
use std::sync::MutexGuard;

use tauri::State;

use crate::{
    objects::{Detail, Kind},
    search::{self, embeddings, Hit, Index, Query},
    AppState,
};

const MAX_QUERY_CHARS: usize = 200;

fn search_index<'a>(
    state: &'a State<'_, AppState>,
) -> Result<MutexGuard<'a, Option<Index>>, String> {
    state
        .search
        .lock()
        .map_err(|_| "Search is unavailable".to_string())
}

/// Saved stories about held stocks that are about the company, not passing mentions.
async fn stories(state: &State<'_, AppState>, tickers: &[String]) -> Vec<crate::objects::Object> {
    let Ok(cache) = state.providers.news_cache().await else {
        return Vec::new();
    };
    tickers
        .iter()
        .flat_map(|ticker| cache.articles(ticker).unwrap_or_default())
        .filter(|article| article.relevance >= crate::providers::news::RELEVANT)
        .map(|article| {
            let story = search::story(&article);
            (story.id.clone(), story)
        })
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_values()
        .collect()
}

/// Brings the index up to date with the graph and saved stories.
async fn current_index(state: &State<'_, AppState>) -> Result<std::path::PathBuf, String> {
    let (graph, directory) = {
        let storage = state.storage()?;
        let directory = storage
            .database_path()
            .parent()
            .ok_or("Local data directory unavailable")?
            .to_owned();
        (storage.graph()?, directory)
    };
    let tickers = graph
        .objects
        .iter()
        .filter(|object| object.kind == Kind::Holding)
        .map(|object| object.title.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let stories = stories(state, &tickers).await;
    let key = format!(
        "{}:{}",
        graph.hash()?,
        crate::database::sha256(
            stories
                .iter()
                .map(|story| story.id.as_str())
                .collect::<Vec<_>>()
                .join(",")
                .as_bytes()
        )
    );
    let mut index = search_index(state)?;
    if index.as_ref().is_none_or(|index| index.key != key) {
        let objects = graph.objects.iter().cloned().chain(stories).collect();
        *index = Some(Index::build(key, objects)?);
    }
    Ok(directory.join("embeddings.sqlite3"))
}

/// Every object matching the query, by words and then by meaning.
#[tauri::command]
pub(crate) async fn search_objects(
    query: Query,
    state: State<'_, AppState>,
) -> Result<Vec<Hit>, String> {
    if query.text.chars().count() > MAX_QUERY_CHARS || query.kinds.len() > Kind::ALL.len() {
        return Err("Invalid search".into());
    }
    let limit = query
        .limit
        .unwrap_or(search::DEFAULT_LIMIT)
        .clamp(1, search::MAX_LIMIT);
    let cache = current_index(&state).await?;
    let mut guard = search_index(&state)?;
    let index = guard.as_mut().ok_or("Search is unavailable")?;
    let words = index.search(&query, limit, None)?;
    if words.len() >= limit || query.text.trim().chars().count() < 3 {
        return Ok(words);
    }
    // Words found too little; meanings fill the rest. The first such search embeds every
    // object once, and later ones read the cache.
    if let Some(texts) = index.meaning_texts() {
        index.set_meanings(embeddings::embed_cached(
            &cache,
            &embeddings::Apple,
            &texts,
        )?);
    }
    let meaning = embeddings::embed_cached(
        &cache,
        &embeddings::Apple,
        &[(0, query.text.trim().to_owned())],
    )?
    .pop()
    .map(|(_, vector)| vector);
    index.search(&query, limit, meaning)
}

fn data_directory(state: &State<'_, AppState>) -> Result<std::path::PathBuf, String> {
    Ok(state
        .storage()?
        .database_path()
        .parent()
        .ok_or("Local data directory unavailable")?
        .to_owned())
}

/// Every file in Brief's data folder, with each database's tables.
#[tauri::command(async)]
pub(crate) fn get_stored_files(
    state: State<'_, AppState>,
) -> Result<Vec<crate::inventory::File>, String> {
    crate::inventory::files(&data_directory(&state)?)
}

/// A page of rows from one table of one of Brief's databases, read-only.
#[tauri::command(async)]
pub(crate) fn get_stored_rows(
    file: String,
    table: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<crate::inventory::Rows, String> {
    if file.len() > 128 || table.len() > 128 || offset > 10_000_000 {
        return Err("Invalid table".into());
    }
    crate::inventory::rows(&data_directory(&state)?, &file, &table, offset)
}

/// One object with its links, facts, and provider record, for its raw view.
#[tauri::command]
pub(crate) async fn get_object(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<Detail>, String> {
    if id.len() > 512 || crate::objects::kind_of(&id).is_none() {
        return Err("Invalid object".into());
    }
    if crate::objects::kind_of(&id) == Some(Kind::Story) {
        current_index(&state).await?;
        let index = search_index(&state)?;
        return Ok(index
            .as_ref()
            .and_then(|index| index.object(&id))
            .map(|object| Detail {
                object: object.clone(),
                links: Vec::new(),
                facts: Vec::new(),
                provider: None,
            }));
    }
    state.storage()?.object(&id)
}
