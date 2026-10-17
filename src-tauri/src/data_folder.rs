//! The readable folder: a one-way copy of the object graph as plain CSV and Markdown, written
//! after changes when the user turns it on. The finance database stays the truth; the folder
//! is never read back. It is written beside itself and swapped in whole, and Brief replaces or
//! erases only a folder carrying its own marker, never one it didn't write.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::objects::{organization_key, Graph, Kind, Object};

pub const FOLDER_NAME: &str = "Brief Data";
const MARKER: &str = ".brief-data";
const MASK: &str = "•••";

/// The folder Brief writes inside the chosen one.
pub fn target(chosen: &Path) -> PathBuf {
    chosen.join(FOLDER_NAME)
}

/// Whether macOS may sync the chosen folder to the cloud: iCloud Drive, Desktop and
/// Documents (which iCloud can take over), and cloud storage providers.
pub fn synced(chosen: &Path, home: &Path) -> bool {
    [
        "Library/Mobile Documents",
        "Library/CloudStorage",
        "Desktop",
        "Documents",
    ]
    .iter()
    .any(|place| chosen.starts_with(home.join(place)))
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

fn csv(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|field| csv_field(field))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Every number in a record as the mask, for a folder written masked.
fn masked(value: &Value) -> Value {
    match value {
        Value::Number(_) => Value::String(MASK.into()),
        Value::Array(items) => Value::Array(items.iter().map(masked).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), masked(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

struct Writer<'a> {
    graph: &'a Graph,
    masked: bool,
    /// Each object's facts' sources, for the sources column.
    sources: BTreeMap<&'a str, BTreeSet<&'a str>>,
    /// The Markdown page names of linkable objects, by id.
    pages: BTreeMap<&'a str, String>,
}

/// Kinds written as Markdown pages that link to each other.
const PAGE_KINDS: [(Kind, &str); 4] = [
    (Kind::Person, "people"),
    (Kind::Organization, "organizations"),
    (Kind::Place, "places"),
    (Kind::Trip, "trips"),
];

/// Kinds written as one CSV each; transactions are split by year.
const TABLE_KINDS: [(Kind, &str); 7] = [
    (Kind::Account, "accounts.csv"),
    (Kind::Holding, "holdings.csv"),
    (Kind::Trade, "trades.csv"),
    (Kind::Document, "documents.csv"),
    (Kind::Statement, "statements.csv"),
    (Kind::Bill, "bills.csv"),
    (Kind::Benefit, "benefits.csv"),
];

impl<'a> Writer<'a> {
    fn new(graph: &'a Graph, masked: bool) -> Self {
        let mut sources = BTreeMap::<&str, BTreeSet<&str>>::new();
        for fact in &graph.facts {
            sources
                .entry(fact.subject.as_str())
                .or_default()
                .extend(fact.sources.iter().map(String::as_str));
        }
        let mut pages = BTreeMap::new();
        let mut taken = BTreeSet::new();
        for object in &graph.objects {
            if PAGE_KINDS.iter().any(|(kind, _)| *kind == object.kind) {
                let base = organization_key(&object.title).unwrap_or_else(|| "untitled".into());
                let mut name = base.clone();
                let mut count = 1;
                while !taken.insert((object.kind, name.clone())) {
                    count += 1;
                    name = format!("{base}-{count}");
                }
                pages.insert(object.id.as_str(), name);
            }
        }
        Self {
            graph,
            masked,
            sources,
            pages,
        }
    }

    fn amount(&self, amount: Option<f64>) -> String {
        match amount {
            Some(_) if self.masked => MASK.into(),
            Some(amount) => format!("{amount:.2}"),
            None => String::new(),
        }
    }

    fn record(&self, object: &Object) -> String {
        let record = if self.masked {
            masked(&object.record)
        } else {
            object.record.clone()
        };
        serde_json::to_string(&record).unwrap_or_default()
    }

    fn row(&self, object: &Object) -> Vec<String> {
        vec![
            object.id.clone(),
            object.title.clone(),
            object.subtitle.clone().unwrap_or_default(),
            object.date.clone().unwrap_or_default(),
            self.amount(object.amount),
            self.sources
                .get(object.id.as_str())
                .map(|sources| sources.iter().copied().collect::<Vec<_>>().join(" "))
                .unwrap_or_default(),
            self.record(object),
        ]
    }

    fn table(&self, objects: impl Iterator<Item = &'a Object>) -> String {
        let header = [
            "id", "title", "subtitle", "date", "amount", "sources", "record",
        ]
        .map(str::to_owned)
        .to_vec();
        let rows = std::iter::once(header)
            .chain(objects.map(|object| self.row(object)))
            .collect::<Vec<_>>();
        csv(&rows)
    }

    /// How a page names another object: a wiki link to its page, or its title.
    fn mention(&self, id: &str) -> String {
        let object = self.graph.objects.iter().find(|object| object.id == id);
        let title = object.map_or(id, |object| object.title.as_str());
        match (self.pages.get(id), object) {
            (Some(page), Some(object)) => {
                let folder = PAGE_KINDS
                    .iter()
                    .find(|(kind, _)| *kind == object.kind)
                    .map_or("", |(_, folder)| folder);
                format!("[[{folder}/{page}|{title}]]")
            }
            _ => {
                let detail = object
                    .map(|object| {
                        [
                            object.date.clone(),
                            object.amount.map(|_| self.amount(object.amount)),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(", ")
                    })
                    .unwrap_or_default();
                if detail.is_empty() {
                    title.to_owned()
                } else {
                    format!("{title} ({detail})")
                }
            }
        }
    }

    fn page(&self, object: &Object) -> String {
        let mut page = String::from("---\n");
        let _ = writeln!(
            page,
            "id: {}",
            serde_json::to_string(&object.id).unwrap_or_default()
        );
        let _ = writeln!(page, "kind: {}", object.kind.name());
        let _ = writeln!(
            page,
            "title: {}",
            serde_json::to_string(&object.title).unwrap_or_default()
        );
        if let Some(date) = &object.date {
            let _ = writeln!(page, "date: {date}");
        }
        page.push_str("---\n\n");
        let _ = writeln!(page, "# {}\n", object.title);
        if let Some(subtitle) = &object.subtitle {
            let _ = writeln!(page, "{subtitle}\n");
        }
        let facts = self
            .graph
            .facts
            .iter()
            .filter(|fact| fact.subject == object.id)
            .collect::<Vec<_>>();
        if !facts.is_empty() {
            page.push_str("## Facts\n\n");
            for fact in facts {
                let value = match &fact.value {
                    Value::String(text) if crate::objects::kind_of(text).is_some() => {
                        self.mention(text)
                    }
                    Value::Number(_) if self.masked => MASK.into(),
                    other => other.to_string(),
                };
                let span = match (&fact.from, &fact.to) {
                    (Some(from), Some(to)) => format!(", {from} to {to}"),
                    (Some(from), None) => format!(", from {from}"),
                    _ => String::new(),
                };
                let _ = writeln!(
                    page,
                    "- {}: {value} ({}{span})",
                    fact.predicate,
                    fact.sources.join(", ")
                );
            }
            page.push('\n');
        }
        let mut linked = self
            .graph
            .links
            .iter()
            .filter_map(|link| {
                if link.to == object.id {
                    Some((link.relation, link.from.as_str()))
                } else if link.from == object.id {
                    Some((link.relation, link.to.as_str()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        linked.sort();
        if !linked.is_empty() {
            page.push_str("## Linked\n\n");
            for (relation, id) in linked {
                let relation = serde_json::to_value(relation)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let _ = writeln!(page, "- {relation}: {}", self.mention(id));
            }
        }
        page
    }

    fn readme(&self, written_at: &str) -> String {
        let mut readme = format!(
            "# Brief Data\n\nA readable copy of what Brief knows, written {written_at}. Brief \
             writes this folder after changes and never reads it back; your finance database \
             is the source of truth. Opened in Obsidian, the pages link to each other.\n\n"
        );
        if self.masked {
            readme.push_str("Amounts are written masked, as `•••`.\n\n");
        }
        readme.push_str(
            "| File | What it holds |\n| --- | --- |\n\
             | `accounts.csv`, `holdings.csv`, `trades.csv` | Accounts, positions, and trades |\n\
             | `transactions/<year>.csv` | Every transaction, by year |\n\
             | `documents.csv`, `statements.csv` | What Brief read from your documents and statements |\n\
             | `bills.csv`, `benefits.csv` | Recurring bills and deposits, and card benefits |\n\
             | `facts.csv` | Every fact: subject, predicate, value, span, sources, tier |\n\
             | `people/`, `organizations/`, `places/`, `trips/` | One page each, linked |\n\n\
             Each CSV has one row per object: its id, title, subtitle, date, amount, the sources \
             of its facts, and its complete record as JSON.\n",
        );
        readme
    }

    fn facts(&self) -> String {
        let mut rows = vec![[
            "subject",
            "predicate",
            "value",
            "from",
            "to",
            "sources",
            "grade",
            "tier",
        ]
        .map(str::to_owned)
        .to_vec()];
        for fact in &self.graph.facts {
            let value = if self.masked && fact.value.is_number() {
                MASK.to_owned()
            } else {
                match &fact.value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                }
            };
            let name = |value: Value| value.as_str().unwrap_or_default().to_owned();
            rows.push(vec![
                fact.subject.clone(),
                fact.predicate.clone(),
                value,
                fact.from.clone().unwrap_or_default(),
                fact.to.clone().unwrap_or_default(),
                fact.sources.join(" "),
                fact.grade
                    .map(|grade| name(serde_json::to_value(grade).unwrap_or_default()))
                    .unwrap_or_default(),
                name(serde_json::to_value(fact.tier).unwrap_or_default()),
            ]);
        }
        csv(&rows)
    }

    /// Every file's path inside the folder, and its contents.
    fn files(&self, written_at: &str) -> Vec<(PathBuf, String)> {
        let of = |kind: Kind| {
            self.graph
                .objects
                .iter()
                .filter(move |object| object.kind == kind)
        };
        let mut files = vec![
            (PathBuf::from("README.md"), self.readme(written_at)),
            (PathBuf::from("facts.csv"), self.facts()),
        ];
        for (kind, name) in TABLE_KINDS {
            files.push((PathBuf::from(name), self.table(of(kind))));
        }
        let mut years = BTreeMap::<String, Vec<&Object>>::new();
        for transaction in of(Kind::Transaction) {
            let year = transaction
                .date
                .as_deref()
                .and_then(|date| date.get(..4))
                .unwrap_or("undated")
                .to_owned();
            years.entry(year).or_default().push(transaction);
        }
        for (year, transactions) in years {
            files.push((
                PathBuf::from("transactions").join(format!("{year}.csv")),
                self.table(transactions.into_iter()),
            ));
        }
        for (kind, folder) in PAGE_KINDS {
            for object in of(kind) {
                if let Some(page) = self.pages.get(object.id.as_str()) {
                    files.push((
                        PathBuf::from(folder).join(format!("{page}.md")),
                        self.page(object),
                    ));
                }
            }
        }
        files
    }
}

/// Whether a folder is one Brief wrote.
fn ours(folder: &Path) -> bool {
    folder.join(MARKER).is_file()
}

/// Writes the graph into `Brief Data` inside `chosen`, replacing the copy Brief wrote before.
pub fn write(chosen: &Path, graph: &Graph, masked: bool, written_at: &str) -> Result<(), String> {
    if !chosen.is_dir() {
        return Err("The chosen folder is missing".into());
    }
    let target = target(chosen);
    if target.exists() && !ours(&target) {
        return Err(format!(
            "A folder named {FOLDER_NAME} that Brief didn't write is already there"
        ));
    }
    let staging = chosen.join(format!(".{FOLDER_NAME}.{}", uuid::Uuid::new_v4()));
    let result = (|| {
        for (path, contents) in Writer::new(graph, masked)
            .files(written_at)
            .into_iter()
            .chain([(
                PathBuf::from(MARKER),
                "Written by Brief. Brief replaces this folder whole; edits here are lost.\n".into(),
            )])
        {
            let path = staging.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|_| "Could not write the data folder")?;
            }
            fs::write(&path, contents).map_err(|_| "Could not write the data folder")?;
        }
        // Swap the new copy in, then remove the old one.
        let previous = chosen.join(format!(".{FOLDER_NAME}.old.{}", uuid::Uuid::new_v4()));
        if target.exists() {
            fs::rename(&target, &previous).map_err(|_| "Could not replace the data folder")?;
        }
        fs::rename(&staging, &target).map_err(|_| "Could not replace the data folder")?;
        if previous.exists() {
            let _ = fs::remove_dir_all(&previous);
        }
        Ok::<_, String>(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// Removes the copy Brief wrote, as erasure and turning the folder off do.
pub fn remove(chosen: &Path) -> Result<(), String> {
    let target = target(chosen);
    if !target.exists() {
        return Ok(());
    }
    if !ours(&target) {
        return Err(format!(
            "{FOLDER_NAME} wasn't written by Brief, so it was left in place"
        ));
    }
    fs::remove_dir_all(&target).map_err(|_| "Could not remove the data folder".to_string())
}

/// The folder's last write, for Settings.
#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "DataFolderStatus")]
pub struct Status {
    /// The folder Brief writes, inside the chosen one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub written_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// The chosen folder may sync to the cloud.
    pub synced: bool,
}

#[cfg(test)]
mod tests;
