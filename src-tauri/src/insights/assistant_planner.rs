//! The assistant's planner: what a question asks, settled in Rust before the on-device model
//! sees it. Relative dates ("last month", "since I moved") become explicit days. Tools are
//! scored by the question's words and the names in the user's ledger. When one tool clearly
//! leads, Rust runs it and the model only phrases the result; otherwise the model is offered
//! the leading tools.
//!
//! Planning exists because the model reads about 4,096 tokens per question, tool definitions
//! included (`OFFERED`). Routing reads words, not the question's embedding: embedding it would
//! add a native call per question that tests can't run, and
//! `the_planner_routes_every_eval_question_and_resolves_its_dates` checks every eval question's
//! route without the model.
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde_json::{json, Value};

use super::{assistant_tools, calendar, View};

/// A date phrase in the question and the days it means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// The words that named it, as the question wrote them (lowercased).
    pub phrase: String,
}

/// What Rust settled about a question.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The tools offered to the model, leading first, at most `OFFERED`.
    pub tools: Vec<&'static str>,
    /// The days the question names, which date-taking tools are given.
    pub range: Option<Range>,
    /// The one query Rust runs itself, when the leading tool is clear and its arguments known.
    pub direct: Option<(&'static str, Value)>,
}

/// Tools whose arguments are a date range.
pub const DATED: [&str; 3] = ["spendingByCategory", "cashFlow", "searchTransactions"];
/// Offered when a question points to no tool.
const GENERAL: [&str; 4] = ["listAccounts", "cashFlow", "searchTransactions", "chapters"];
/// Tool definitions offered per question; each takes part of the model's context.
const OFFERED: usize = 4;
/// How far the leading tool's score must exceed the next one's for Rust to run it itself.
const CLEAR_LEAD: u32 = 2;
/// Words a question the ledger can't answer tends to use: predictions and scores Brief doesn't
/// hold. Such a question is never run directly, so the model can call it unavailable.
const OUT_OF_SCOPE: [&str; 8] = [
    "score",
    "predict",
    "prediction",
    "go up",
    "go down",
    "should i",
    "will the",
    "forecast",
];

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// The question lowercased, its words separated by single spaces and padded with one on each
/// side, so a phrase matches only as whole words.
fn normal(text: &str) -> String {
    let words = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    format!(" {words} ")
}

fn has_phrase(normal: &str, phrase: &str) -> bool {
    normal.contains(&format!(" {phrase} "))
}

fn month_number(word: &str) -> Option<u32> {
    if word == "sept" {
        return Some(9);
    }
    MONTHS
        .iter()
        .position(|month| *month == word)
        .map(|index| index as u32 + 1)
}

fn year_number(word: &str) -> Option<i32> {
    (word.len() == 4)
        .then(|| word.parse::<i32>().ok())
        .flatten()
        .filter(|year| (1990..=2100).contains(year))
}

fn count_number(word: &str) -> Option<u32> {
    const WORDS: [&str; 12] = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
        "twelve",
    ];
    word.parse::<u32>()
        .ok()
        .filter(|count| (1..=3_660).contains(count))
        .or_else(|| {
            WORDS
                .iter()
                .position(|known| *known == word)
                .map(|index| index as u32 + 1)
        })
}

fn month_start(year: i32, month: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, 1)
}

fn month_end(year: i32, month: u32) -> Option<NaiveDate> {
    Some(month_start(year, month)?.checked_add_months(Months::new(1))? - Duration::days(1))
}

fn quarter_span(year: i32, quarter: u32) -> Option<(NaiveDate, NaiveDate)> {
    let first = (quarter - 1) * 3 + 1;
    Some((month_start(year, first)?, month_end(year, first + 2)?))
}

/// The latest year in which `month` has begun by `today`.
fn latest_year_of(month: u32, today: NaiveDate) -> i32 {
    if month <= today.month() {
        today.year()
    } else {
        today.year() - 1
    }
}

/// The calendar period of `unit` holding `day`.
fn period_of(unit: &str, day: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    match unit {
        "week" => {
            let start = day - Duration::days(i64::from(day.weekday().num_days_from_monday()));
            Some((start, start + Duration::days(6)))
        }
        "month" => Some((
            month_start(day.year(), day.month())?,
            month_end(day.year(), day.month())?,
        )),
        "quarter" => quarter_span(day.year(), (day.month() - 1) / 3 + 1),
        "year" => Some((
            NaiveDate::from_ymd_opt(day.year(), 1, 1)?,
            NaiveDate::from_ymd_opt(day.year(), 12, 31)?,
        )),
        _ => None,
    }
}

/// `count` units back from `today`, ending today: the trailing window "the past 90 days" means.
fn trailing(unit: &str, count: u32, today: NaiveDate) -> Option<NaiveDate> {
    let unit = unit.trim_end_matches('s');
    let start = match unit {
        "day" => today - Duration::days(i64::from(count)),
        "week" => today - Duration::days(7 * i64::from(count)),
        "month" => today.checked_sub_months(Months::new(count))?,
        "quarter" => today.checked_sub_months(Months::new(3 * count))?,
        "year" => today.checked_sub_months(Months::new(12 * count))?,
        _ => return None,
    };
    Some(start + Duration::days(1))
}

const UNITS: [&str; 4] = ["week", "month", "quarter", "year"];

/// The days a question's date phrase means, given today and the user's chapters: "last month",
/// "this year", "since March", "the past 90 days", "in 2025", "yesterday", "Q3 2026", "the
/// first quarter", "September 2026", and "since I moved" or "since I started my job" when the
/// evidence holds that chapter. A question with no such phrase, or one this can't settle, has
/// none, and the model reads the question as written.
pub fn resolve_dates(
    question: &str,
    today: NaiveDate,
    evidence: Option<&crate::evidence::Evidence>,
) -> Option<Range> {
    let text = normal(question);
    let words = text.split_whitespace().collect::<Vec<_>>();
    let at = |index: usize| words.get(index).copied().unwrap_or("");
    let range = |start: NaiveDate, end: NaiveDate, phrase: &[&str]| {
        Some(Range {
            start,
            end,
            phrase: phrase.join(" "),
        })
    };
    // "since …": a month, a year, or a chapter.
    if let Some(index) = words.iter().position(|word| *word == "since") {
        let next = at(index + 1);
        if let Some(month) = month_number(next) {
            if let Some(year) = year_number(at(index + 2)) {
                return range(month_start(year, month)?, today, &words[index..index + 3]);
            }
            return range(
                month_start(latest_year_of(month, today), month)?,
                today,
                &words[index..index + 2],
            );
        }
        if let Some(year) = year_number(next) {
            return range(
                NaiveDate::from_ymd_opt(year, 1, 1)?,
                today,
                &words[index..index + 2],
            );
        }
        // The fewest words after "since" that name a chapter ("I moved", "joining"), then
        // the words that go on naming it ("my job", the employer's name).
        if let Some(evidence) = evidence {
            let rest = &words[index + 1..words.len().min(index + 6)];
            let found = (1..=rest.len()).find_map(|count| {
                assistant_tools::chapter_named(evidence, &rest[..count].join(" "))
                    .map(|chapter| (count, chapter))
            });
            if let Some((mut count, chapter)) = found {
                let title = normal(&chapter.title);
                while count < rest.len()
                    && (["my", "job", "new", "the", "here", "home", "work"].contains(&rest[count])
                        || has_phrase(&title, rest[count]))
                {
                    count += 1;
                }
                return range(chapter.start, today, &words[index..=index + count]);
            }
        }
    }
    for (index, word) in words.iter().enumerate() {
        let next = at(index + 1);
        let after = at(index + 2);
        // "the past 90 days", "last 3 months"
        if matches!(*word, "past" | "last" | "previous") {
            if let Some(count) = count_number(next) {
                if let Some(start) = trailing(after, count, today) {
                    return range(start, today, &words[index..index + 3]);
                }
            }
            // "the past year", "the last month", "past week": a trailing window.
            let unit = next;
            if UNITS.contains(&unit) && (*word == "past" || (index > 0 && at(index - 1) == "the")) {
                return range(trailing(unit, 1, today)?, today, &words[index..index + 2]);
            }
            // "last month", "previous year": the calendar period before this one.
            if UNITS.contains(&unit) {
                let (this_start, _) = period_of(unit, today)?;
                let (start, end) = period_of(unit, this_start - Duration::days(1))?;
                return range(start, end, &words[index..index + 2]);
            }
        }
        if *word == "this" && UNITS.contains(&next) {
            let (start, _) = period_of(next, today)?;
            return range(start, today, &words[index..index + 2]);
        }
        if (*word == "year" || *word == "month") && next == "to" && after == "date" {
            let (start, _) = period_of(word, today)?;
            return range(start, today, &words[index..index + 3]);
        }
        if *word == "ytd" {
            let (start, _) = period_of("year", today)?;
            return range(start, today, &words[index..index + 1]);
        }
        if *word == "yesterday" {
            let day = today - Duration::days(1);
            return range(day, day, &words[index..index + 1]);
        }
        if *word == "today" {
            return range(today, today, &words[index..index + 1]);
        }
        // "q3 2026", "the third quarter of 2025", "the first quarter"
        let quarter = match *word {
            "q1" | "first" => Some(1),
            "q2" | "second" => Some(2),
            "q3" | "third" => Some(3),
            "q4" | "fourth" => Some(4),
            _ => None,
        }
        .filter(|_| word.starts_with('q') || next == "quarter");
        if let Some(quarter) = quarter {
            let skip = if word.starts_with('q') { 1 } else { 2 };
            let (year, used) = match (year_number(at(index + skip)), at(index + skip)) {
                (Some(year), _) => (Some(year), skip + 1),
                (None, "of") => (year_number(at(index + skip + 1)), skip + 2),
                _ => (None, skip),
            };
            let year = year.unwrap_or_else(|| {
                let (start, _) = quarter_span(today.year(), quarter).unwrap_or((today, today));
                if start <= today {
                    today.year()
                } else {
                    today.year() - 1
                }
            });
            let (start, end) = quarter_span(year, quarter)?;
            return range(start, end, &words[index..(index + used).min(words.len())]);
        }
        // "September 2026", "in September"; "may" only as a month with a year or "in".
        if let Some(month) = month_number(word) {
            if let Some(year) = year_number(next) {
                return range(
                    month_start(year, month)?,
                    month_end(year, month)?,
                    &words[index..index + 2],
                );
            }
            let introduced = index > 0 && matches!(at(index - 1), "in" | "during" | "for");
            if *word != "may" || introduced {
                let year = latest_year_of(month, today);
                return range(
                    month_start(year, month)?,
                    month_end(year, month)?,
                    &words[index..index + 1],
                );
            }
        }
        if let Some(year) = year_number(word) {
            return range(
                NaiveDate::from_ymd_opt(year, 1, 1)?,
                NaiveDate::from_ymd_opt(year, 12, 31)?,
                &words[index..index + 1],
            );
        }
    }
    None
}

/// Words that name nothing on their own, left out when names from the ledger are matched.
const GENERIC: [&str; 18] = [
    "the", "and", "my", "account", "accounts", "card", "checking", "payment", "transfer", "to",
    "from", "inc", "llc", "lived", "fee", "fees", "total", "market",
];

/// Where a name in the user's ledger comes from, which decides the tools it points to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Account,
    Card,
    Merchant,
    Category,
    Holding,
    Trip,
    Chapter,
}

impl Kind {
    /// The tools a name of this kind points to, and how strongly.
    fn tools(self) -> &'static [(&'static str, u32)] {
        match self {
            Kind::Account => &[("listAccounts", 2)],
            Kind::Card => &[("creditCards", 2), ("listAccounts", 1)],
            Kind::Merchant => &[("merchantHistory", 2), ("searchTransactions", 1)],
            Kind::Category => &[("spendingByCategory", 1)],
            Kind::Holding => &[("topHoldings", 2)],
            // Spending "in Chicago" is a trip's, not a period's.
            Kind::Trip => &[("trips", 3)],
            Kind::Chapter => &[("chapters", 2)],
        }
    }
}

/// Names in the user's ledger: account names, merchants, categories, holdings' tickers
/// and names, trip cities, and chapter titles and states.
struct Names {
    entries: Vec<(Kind, String)>,
}

impl Names {
    fn of(view: &View) -> Self {
        let mut entries = Vec::new();
        for account in &view.snapshot.accounts {
            if account.id != "all" {
                let name = view.account_name(&account.id).unwrap_or(&account.name);
                let kind = if account.r#type == "credit" {
                    Kind::Card
                } else {
                    Kind::Account
                };
                entries.push((kind, name.to_owned()));
            }
        }
        for t in &view.snapshot.transactions {
            entries.push((Kind::Merchant, t.merchant.clone()));
            entries.push((Kind::Category, t.category.clone()));
        }
        for holding in &view.snapshot.holdings {
            entries.push((Kind::Holding, holding.ticker.clone()));
            entries.push((Kind::Holding, holding.name.clone()));
        }
        if let Some(insights) = view.insights.as_ref() {
            for trip in &insights.trips.trips {
                let city = trip.destination.split(',').next().unwrap_or_default();
                entries.push((Kind::Trip, city.to_owned()));
            }
            for chapter in &insights.evidence.chapters {
                entries.push((Kind::Chapter, chapter.title.clone()));
                if let Some(state) = chapter
                    .state
                    .as_deref()
                    .and_then(crate::evidence::chapters::state_name)
                {
                    entries.push((Kind::Chapter, state.to_owned()));
                }
            }
        }
        entries.sort();
        entries.dedup();
        Self { entries }
    }

    /// Each kind's distinctive words the question uses. A word that names things of more than
    /// one kind (an employer that is also a merchant) tells the tools apart no better than none,
    /// unless the question writes out a whole name of one kind ("Example Card").
    fn matched(&self, text: &str) -> Vec<(Kind, Vec<String>)> {
        let mut kinds = std::collections::BTreeMap::<String, Vec<Kind>>::new();
        // A whole name counts for its kind only when no name of another kind holds it too
        // ("Example Analytics" the merchant and "Example Analytics LLC" the job).
        let whole = self
            .entries
            .iter()
            .filter(|(kind, name)| {
                let name = normal(name);
                name.split_whitespace().count() > 1
                    && has_phrase(text, name.trim())
                    && !self.entries.iter().any(|(other, known)| {
                        other != kind && has_phrase(&normal(known), name.trim())
                    })
            })
            .collect::<Vec<_>>();
        for (kind, name) in &whole {
            for word in normal(name).split_whitespace() {
                if word.len() > 2 && !GENERIC.contains(&word) {
                    kinds.insert(word.to_owned(), vec![*kind]);
                }
            }
        }
        let locked = kinds
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        for (kind, name) in &self.entries {
            if whole.iter().any(|(_, known)| known == name) {
                continue;
            }
            for word in normal(name).split_whitespace() {
                if word.len() > 2
                    && !GENERIC.contains(&word)
                    && has_phrase(text, word)
                    && !locked.contains(word)
                {
                    let known = kinds.entry(word.to_owned()).or_default();
                    if !known.contains(kind) {
                        known.push(*kind);
                    }
                }
            }
        }
        let mut found = Vec::<(Kind, Vec<String>)>::new();
        // In the question's order, so a name built from them reads as the question wrote it.
        for word in text.split_whitespace() {
            let Some([kind]) = kinds.get(word).map(Vec::as_slice) else {
                continue;
            };
            match found.iter_mut().find(|(known, _)| known == kind) {
                Some((_, words)) if !words.iter().any(|known| known == word) => {
                    words.push(word.to_owned())
                }
                Some(_) => {}
                None => found.push((*kind, vec![word.to_owned()])),
            }
        }
        found
    }

    /// The longest name of `kind` the question writes out in full.
    fn whole(&self, kind: Kind, text: &str) -> Option<&str> {
        self.entries
            .iter()
            .filter(|(known, name)| *known == kind && has_phrase(text, normal(name).trim()))
            .map(|(_, name)| name.as_str())
            .max_by_key(|name| name.len())
    }
}

/// The question's text with its resolved date phrase taken out, so "since I moved" in a
/// spending question points to spending, not to chapters.
fn without_dates(question: &str, range: Option<&Range>) -> String {
    let text = normal(question);
    match range {
        Some(range) if !range.phrase.is_empty() => {
            text.replacen(&format!(" {} ", range.phrase), " ", 1)
        }
        _ => text,
    }
}

/// Each tool's score for a question, leading first: for each hint word it uses, two when no
/// other tool lists that word and one when others do; two for each hint phrase; and, for each
/// kind of name from the user's ledger the question uses, that kind's weight for the tool
/// (`Kind::tools`). Ties keep the order of `assistant_tools::TOOLS`.
pub fn scores(question: &str, view: &View, range: Option<&Range>) -> Vec<(&'static str, u32)> {
    let text = without_dates(question, range);
    let words = text.split_whitespace().collect::<Vec<_>>();
    let matched = Names::of(view).matched(&text);
    let shared = |hint: &str| {
        assistant_tools::TOOLS
            .iter()
            .filter(|(_, hints)| hints.contains(&hint))
            .count()
            > 1
    };
    let mut scored = assistant_tools::TOOLS
        .iter()
        .map(|(name, hints)| {
            let hinted = hints
                .iter()
                .map(|hint| {
                    if hint.contains(' ') {
                        2 * u32::from(has_phrase(&text, hint))
                    } else {
                        let uses = words.iter().filter(|word| *word == hint).count() as u32;
                        uses * if shared(hint) { 1 } else { 2 }
                    }
                })
                .sum::<u32>();
            let named = matched
                .iter()
                .filter_map(|(kind, _)| {
                    kind.tools()
                        .iter()
                        .find(|(tool, _)| tool == name)
                        .map(|(_, weight)| *weight)
                })
                .sum::<u32>();
            (*name, hinted + named)
        })
        .collect::<Vec<_>>();
    // A stable sort keeps `TOOLS` order among equal scores.
    scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
    scored
}

/// The arguments the leading tool takes when what the question names settles them, else none.
fn arguments(tool: &str, text: &str, range: Option<&Range>, view: &View) -> Option<Value> {
    match tool {
        "listAccounts" | "monthPlan" | "creditCards" | "taxSummary" | "subscriptions"
        | "dividendIncome" | "explanations" | "topHoldings" | "trips" | "upcomingActivity" => {
            Some(json!({}))
        }
        "cashFlow" | "spendingByCategory" => range.map(
            |range| json!({ "start": calendar::iso(range.start), "end": calendar::iso(range.end) }),
        ),
        "merchantHistory" => Names::of(view)
            .whole(Kind::Merchant, text)
            .map(|merchant| json!({ "merchant": merchant })),
        // "When did I …" asks for one chapter's dates, named by the ledger's own words or by
        // a move or a job; any other question reads every chapter.
        "chapters" if text.starts_with(" when ") => {
            let named = Names::of(view)
                .matched(text)
                .into_iter()
                .find(|(kind, _)| *kind == Kind::Chapter)
                .map(|(_, words)| words.join(" "))
                .or_else(|| {
                    ["moved", "move", "started", "joined"]
                        .into_iter()
                        .find(|word| has_phrase(text, word))
                        .map(str::to_owned)
                })?;
            Some(json!({ "name": named }))
        }
        "chapters" => Some(json!({})),
        _ => None,
    }
}

/// Plans a question: its dates, the tools to offer, and the query Rust runs itself when the
/// leading tool is clear (it leads the next by `CLEAR_LEAD`), its arguments are settled, and
/// the question isn't one the ledger can't answer.
pub fn plan(question: &str, view: &View, today: NaiveDate) -> Plan {
    let range = resolve_dates(
        question,
        today,
        view.insights.as_ref().map(|insights| &insights.evidence),
    );
    let text = without_dates(question, range.as_ref());
    let scored = scores(question, view, range.as_ref());
    let mut tools = scored
        .iter()
        .filter(|(_, score)| *score > 0)
        .take(OFFERED)
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    if tools.is_empty() {
        tools = GENERAL.to_vec();
    }
    let lead = scored.first().map_or(0, |(_, score)| *score);
    let next = scored.get(1).map_or(0, |(_, score)| *score);
    let clear = lead >= CLEAR_LEAD && lead - next >= CLEAR_LEAD;
    let answerable = !OUT_OF_SCOPE.iter().any(|phrase| has_phrase(&text, phrase));
    let direct = (clear && answerable)
        .then(|| {
            let tool = scored[0].0;
            arguments(tool, &text, range.as_ref(), view).map(|arguments| (tool, arguments))
        })
        .flatten();
    Plan {
        tools,
        range,
        direct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn resolved(question: &str) -> Option<(String, String)> {
        resolve_dates(question, day("2026-10-17"), None)
            .map(|range| (calendar::iso(range.start), calendar::iso(range.end)))
    }

    fn span(start: &str, end: &str) -> Option<(String, String)> {
        Some((start.into(), end.into()))
    }

    #[test]
    fn relative_dates_resolve_from_a_pinned_today() {
        // 2026-10-17 is a Saturday.
        assert_eq!(
            resolved("What did I spend last month?"),
            span("2026-09-01", "2026-09-30")
        );
        assert_eq!(
            resolved("Income this year?"),
            span("2026-01-01", "2026-10-17")
        );
        assert_eq!(
            resolved("Spending since March"),
            span("2026-03-01", "2026-10-17")
        );
        assert_eq!(
            resolved("Spending since November"),
            span("2025-11-01", "2026-10-17")
        );
        assert_eq!(
            resolved("in the past 90 days"),
            span("2026-07-20", "2026-10-17")
        );
        assert_eq!(resolved("in 2025"), span("2025-01-01", "2025-12-31"));
        assert_eq!(resolved("yesterday"), span("2026-10-16", "2026-10-16"));
        assert_eq!(resolved("Q1 2026"), span("2026-01-01", "2026-03-31"));
        assert_eq!(
            resolved("the third quarter"),
            span("2026-07-01", "2026-09-30")
        );
        assert_eq!(
            resolved("the fourth quarter of 2025"),
            span("2025-10-01", "2025-12-31")
        );
        assert_eq!(resolved("last quarter"), span("2026-07-01", "2026-09-30"));
        assert_eq!(resolved("last week"), span("2026-10-05", "2026-10-11"));
        assert_eq!(resolved("this week"), span("2026-10-12", "2026-10-17"));
        assert_eq!(
            resolved("over the last year"),
            span("2025-10-18", "2026-10-17")
        );
        assert_eq!(resolved("last year"), span("2025-01-01", "2025-12-31"));
        assert_eq!(
            resolved("in September 2026"),
            span("2026-09-01", "2026-09-30")
        );
        assert_eq!(resolved("in December"), span("2025-12-01", "2025-12-31"));
        assert_eq!(resolved("year to date"), span("2026-01-01", "2026-10-17"));
        // "may" is a month only with a year or "in".
        assert_eq!(resolved("May I see my balance?"), None);
        assert_eq!(resolved("in May"), span("2026-05-01", "2026-05-31"));
        // Nothing to resolve stays unresolved.
        assert_eq!(resolved("What is my checking balance?"), None);
        assert_eq!(resolved("since then"), None);
    }

    #[test]
    fn every_hint_can_match_a_question() {
        for (tool, hints) in assistant_tools::TOOLS {
            for hint in hints {
                assert_eq!(
                    normal(hint).trim(),
                    *hint,
                    "{tool}'s hint {hint:?} is lowercase whole words"
                );
                assert!(
                    has_phrase(&normal(&format!("Is {hint}?")), hint),
                    "{tool}'s hint {hint:?} matches"
                );
            }
        }
    }
}
