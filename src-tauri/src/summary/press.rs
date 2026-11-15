//! The press about each holding: how the stories about it read (positive, negative, mixed, or
//! neutral, as a model read each one, `providers::reading::read_stories`) over the period beside
//! the weeks before it, so a turn in its coverage shows against its own usual. Only stories
//! about the company count (`news::RELEVANT` or more); a story no model read is counted apart.
//! It is the press's view, labeled as read by a model, and never a forecast.
use super::*;
use crate::providers::reading::Stance;

/// A holding's press: its stories since the period began and the stances read in them, beside
/// how the stories saved before the period read.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Press {
    pub ticker: String,
    /// Stories about the company since the period began, and how many a model read.
    pub stories: usize,
    pub read: usize,
    pub positive: usize,
    pub negative: usize,
    pub mixed: usize,
    pub neutral: usize,
    /// Stories saved from before the period that a model read, and the share of them that read
    /// as negative and as positive, in percent.
    pub before_read: usize,
    pub before_negative_pct: Option<f64>,
    pub before_positive_pct: Option<f64>,
}

/// The press of each holding with stories since `since`, or of `ticker` alone.
pub(super) fn press(
    evidence: &[Evidence],
    stances: &BTreeMap<String, BTreeMap<String, Stance>>,
    since: DateTime<Utc>,
    ticker: Option<&str>,
) -> Vec<Press> {
    let since = since.to_rfc3339();
    evidence
        .iter()
        .filter(|holding| ticker.is_none_or(|ticker| ticker == holding.ticker))
        .filter_map(|holding| {
            let read = stances.get(&holding.ticker);
            let about = holding.articles.iter().filter(|article| {
                article.scope == NewsScope::Company && article.relevance >= news::RELEVANT
            });
            let (during, before): (Vec<_>, Vec<_>) =
                about.partition(|article| article.created_at >= since);
            if during.is_empty() {
                return None;
            }
            let stance = |article: &&MarketNewsArticle| {
                read.and_then(|read| read.get(&article.url)).copied()
            };
            let count = |stances: &[Stance], wanted: Stance| {
                stances.iter().filter(|stance| **stance == wanted).count()
            };
            let now: Vec<Stance> = during.iter().filter_map(stance).collect();
            let earlier: Vec<Stance> = before.iter().filter_map(stance).collect();
            let share = |wanted: Stance| {
                (!earlier.is_empty()).then(|| {
                    (count(&earlier, wanted) as f64 / earlier.len() as f64 * 1_000.0).round() / 10.0
                })
            };
            Some(Press {
                ticker: holding.ticker.clone(),
                stories: during.len(),
                read: now.len(),
                positive: count(&now, Stance::Positive),
                negative: count(&now, Stance::Negative),
                mixed: count(&now, Stance::Mixed),
                neutral: count(&now, Stance::Neutral),
                before_read: earlier.len(),
                before_negative_pct: share(Stance::Negative),
                before_positive_pct: share(Stance::Positive),
            })
        })
        .collect()
}
