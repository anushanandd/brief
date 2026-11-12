//! Fixtures stay synthetic: every masked account number in the repository's sources, tests,
//! and seed data uses one of the obviously fake masks, so a real one pasted into a fixture
//! fails here before it is committed.
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// The masks a fixture may use.
const ALLOWED: [&str; 4] = ["0000", "1234", "5678", "9999"];

/// Masked account numbers as statements, institutions, and Plaid print them, each capturing
/// the four digits: `XXXXX1234`, `••1234`, `...1234`, `ending in 1234`, `"mask": "1234"`, and
/// an account name's `Checking - 1234"`.
const PATTERNS: [&str; 5] = [
    r"(?:X{3,}|x{3,}|\*{2,}|•+)\s*-?(\d{4})\b",
    r"(?:\.\.\.|…)\s*-?(\d{4})\b",
    r"(?i)\bending(?: in)? (\d{4})(?:[^\d-]|$)",
    r#""mask"\s*:\s*"(\d{4})""#,
    r#"[A-Za-z] -\s?(\d{4})["\\]"#,
];

fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !matches!(
                name.as_ref(),
                "node_modules" | "bindings" | "target" | "dist"
            ) {
                sources(&path, found);
            }
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("rs" | "ts" | "tsx" | "json" | "jsonl" | "csv" | "txt")
        ) {
            found.push(path);
        }
    }
}

#[test]
fn masked_account_numbers_in_the_repository_are_synthetic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let patterns = PATTERNS.map(|pattern| Regex::new(pattern).unwrap());
    let mut files = Vec::new();
    for directory in ["src", "src-tauri/src"] {
        sources(&root.join(directory), &mut files);
    }
    assert!(files.len() > 100, "the scan found the sources");
    let this = Path::new(file!()).file_name().unwrap();
    let mut hits = Vec::new();
    for file in files.iter().filter(|file| file.file_name() != Some(this)) {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            for pattern in &patterns {
                for found in pattern.captures_iter(line) {
                    if !ALLOWED.contains(&&found[1]) {
                        hits.push(format!(
                            "{}:{}: {}",
                            file.strip_prefix(&root).unwrap_or(file).display(),
                            number + 1,
                            &found[0]
                        ));
                    }
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "masks other than {ALLOWED:?}:\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_patterns_find_masks_and_leave_ordinary_numbers() {
    let patterns = PATTERNS.map(|pattern| Regex::new(pattern).unwrap());
    let digits = |line: &str| {
        patterns
            .iter()
            .flat_map(|pattern| pattern.captures_iter(line))
            .map(|found| found[1].to_owned())
            .collect::<Vec<_>>()
    };
    for line in [
        "TRANSFER FROM XXXXX4321 REF",
        "Card ••4321",
        "Everyday Checking ...4321",
        "card ending in 4321.",
        r#""mask": "4321""#,
        r#""name": "Checking - 4321""#,
        r"Account Activity for Checking -4321\n",
    ] {
        assert_eq!(digits(line), ["4321"], "{line}");
    }
    for line in [
        "the quarter ending 2026-12-31",
        "let start = now - 1800;",
        r#""amount": -1200.5"#,
        "4321 shares",
    ] {
        assert!(digits(line).is_empty(), "{line}");
    }
}
