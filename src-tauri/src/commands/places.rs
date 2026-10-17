//! Finding the businesses in-person purchases were made at, when the user asks: Brief reads
//! Overture's places for each city with purchases not searched yet, keeps the candidates near
//! each, and the view matches from them. Only the cities' areas leave the Mac.
use std::collections::BTreeMap;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::{places, AppState};

#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PlacesProgress")]
struct Progress {
    done: usize,
    total: usize,
}

#[tauri::command]
pub(crate) async fn find_places(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let _running = state
        .places
        .try_lock()
        .map_err(|_| "Brief is already finding places")?;
    let (needs, path) = {
        let storage = state.storage()?;
        let view = storage.read_view()?;
        let needs = view
            .snapshot
            .transactions
            .iter()
            .filter_map(places::matching::need)
            .collect::<Vec<_>>();
        (needs, storage.places_path.clone())
    };
    let searched = places::cache::searched(&path, chrono::Utc::now())?;
    // One read per area, for every search in it.
    let mut areas = Vec::<(
        places::overture::Area,
        BTreeMap<String, places::matching::Need>,
    )>::new();
    for need in needs
        .into_iter()
        .filter(|need| !searched.contains(&need.key))
    {
        match areas.iter_mut().find(|(area, _)| *area == need.area) {
            Some((_, group)) => {
                group.insert(need.key.clone(), need);
            }
            None => areas.push((need.area, BTreeMap::from([(need.key.clone(), need)]))),
        }
    }
    if areas.is_empty() {
        return Ok(());
    }
    let total = areas.len();
    let _ = app.emit("places-progress", Progress { done: 0, total });
    let client = state.providers.http();
    let release = places::overture::latest(client).await?;
    for (done, (area, group)) in areas.into_iter().enumerate() {
        let found = places::overture::read(client, &release, area).await?;
        let now = chrono::Utc::now();
        for (key, need) in group {
            let candidates = places::matching::candidates(&need, &found);
            places::cache::save(&path, &release.id, &key, &candidates, now)?;
        }
        // Each city's matches show as soon as they're found.
        let _ = app.emit("finance-snapshot-updated", ());
        let _ = app.emit(
            "places-progress",
            Progress {
                done: done + 1,
                total,
            },
        );
    }
    Ok(())
}
