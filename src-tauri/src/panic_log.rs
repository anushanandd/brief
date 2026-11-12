//! A small record of panics, so a failure that a command survived is still seen.
//!
//! A panic in one command unwinds to Tauri, which reports it as that command's error; locks
//! it poisoned answer "unavailable" from then on. The hook installed at launch appends the
//! time, thread, message, and location of each panic to `panics.log` in the app data folder,
//! keeping the latest `KEPT`, and Data health lists them. Nothing else from the panic is
//! written: no payload beyond its message, and no financial records.
use std::fs;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) const FILE: &str = "panics.log";
/// The entries kept; older ones are dropped as new ones arrive.
const KEPT: usize = 20;
/// The longest message kept, in characters.
const MESSAGE_CHARACTERS: usize = 300;

static PATH: OnceLock<PathBuf> = OnceLock::new();

/// Records each panic in `directory` from now on, after the default hook has run.
pub fn install(directory: &Path) {
    let _ = fs::create_dir_all(directory);
    let path = directory.join(FILE);
    if PATH.set(path.clone()).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        append(&path, &entry(info, chrono::Utc::now()));
    }));
}

/// The latest recorded panics, newest first.
pub fn recent() -> Vec<String> {
    PATH.get().map_or_else(Vec::new, |path| read(path))
}

fn entry(info: &PanicHookInfo<'_>, now: chrono::DateTime<chrono::Utc>) -> String {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("panic without a message");
    let thread = std::thread::current();
    let location = info
        .location()
        .map_or_else(|| "unknown".to_owned(), ToString::to_string);
    line(
        &now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        thread.name().unwrap_or("unnamed"),
        message,
        &location,
    )
}

fn line(time: &str, thread: &str, message: &str, location: &str) -> String {
    let message = message
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MESSAGE_CHARACTERS)
        .collect::<String>();
    format!("{time} [{thread}] {message} at {location}")
}

/// Appends `entry`, keeping the latest `KEPT`. Failing to write is never a second panic.
fn append(path: &Path, entry: &str) {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut lines = existing.lines().collect::<Vec<_>>();
    lines.push(entry);
    let keep = &lines[lines.len().saturating_sub(KEPT)..];
    let _ = fs::write(path, keep.join("\n") + "\n");
    // Owner-only, like every file in the data folder.
    let _ = crate::database::secure_file(path);
}

fn read(path: &Path) -> Vec<String> {
    let mut lines = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.reverse();
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_latest_entries_one_line_each_newest_first() {
        let directory = std::env::temp_dir().join(format!("brief-panics-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(FILE);
        let _ = fs::remove_file(&path);
        assert!(read(&path).is_empty(), "no file reads as no panics");
        for index in 0..KEPT + 5 {
            append(
                &path,
                &line(
                    "2026-11-11T12:00:00Z",
                    "main",
                    &format!("failure {index}\nsecond line"),
                    "src/lib.rs:1:1",
                ),
            );
        }
        let entries = read(&path);
        assert_eq!(entries.len(), KEPT);
        assert_eq!(
            entries[0],
            format!(
                "2026-11-11T12:00:00Z [main] failure {} second line at src/lib.rs:1:1",
                KEPT + 4
            )
        );
        assert!(entries.last().unwrap().contains("failure 5 "));
        let long = line("t", "main", &"x".repeat(1_000), "here");
        assert_eq!(long.matches('x').count(), MESSAGE_CHARACTERS);
        fs::remove_dir_all(&directory).unwrap();
    }
}
