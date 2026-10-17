//! Ranking by visible components, compared in order.
use super::*;

/// The visible components an event is ranked by, compared in order and shown as labels;
/// never collapsed into a score.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Rank {
    /// The event's dollars on the portfolio, in tenths of a percent of it: 0 under 0.1%, then
    /// 0.1%, 0.5%, 1%, and 2% and up.
    pub dollar_bucket: u8,
    /// The holding's own move that session: 0 under one typical day, then one, two, three.
    pub z_bucket: u8,
    /// How much its kind tends to move prices, from event studies: restatements, bankruptcies,
    /// and impairments 3; earnings, deals, leadership, and dividend changes 2; the rest 1.
    pub prior: u8,
    /// The first report of its cluster, or not a report at all.
    pub novel: bool,
}

pub(in crate::summary) fn type_prior(event: &Event) -> u8 {
    let codes: Vec<&str> = event.subtype.as_deref().unwrap_or("").split(',').collect();
    let high = [
        "1.03", "2.04", "2.06", "4.01", "4.02", "1.05", "2.05", "3.01",
    ];
    let medium = ["2.02", "5.02", "1.01", "1.02", "2.01", "5.01"];
    match event.family {
        Family::Disclosure if codes.iter().any(|code| high.contains(code)) => 3,
        Family::Disclosure if codes.iter().any(|code| medium.contains(code)) => 2,
        Family::News
            if matches!(
                event.subtype.as_deref(),
                Some("earnings" | "deal" | "leadership" | "legal")
            ) =>
        {
            2
        }
        Family::CorporateAction if event.kind == "cashDividend" => 2,
        _ => 1,
    }
}

pub(in crate::summary) fn rank(event: &Event, links: &Links, portfolio: f64) -> Rank {
    let share = event
        .effect
        .filter(|_| portfolio > 0.0)
        .map_or(0.0, |dollars| dollars.abs() / portfolio * 100.0);
    let dollar_bucket = [0.1, 0.5, 1.0, 2.0]
        .iter()
        .filter(|threshold| share >= **threshold)
        .count() as u8;
    let z_bucket = event.z.map_or(0, |z| (z.abs().floor() as u8).min(3));
    Rank {
        dollar_bucket,
        z_bucket,
        prior: type_prior(event),
        novel: event.family != Family::News || links.first.contains(&event.id),
    }
}

impl Rank {
    /// Larger first: dollars, then the move, then the kind, then novelty.
    pub fn key(&self) -> (u8, u8, u8, bool) {
        (self.dollar_bucket, self.z_bucket, self.prior, self.novel)
    }
}
