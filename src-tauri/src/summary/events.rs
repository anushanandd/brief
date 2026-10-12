//! How much each held company was written about in the period. Which events mattered is the
//! event log's (`eventlog`).

use super::*;

/// Each stock's stories at least `RELEVANT` to it in the period, and the most relevant.
pub(super) fn coverage(evidence: &[Evidence], period: &Period) -> Vec<Coverage> {
    evidence
        .iter()
        .map(|holding| {
            let relevant: Vec<u8> = holding
                .articles
                .iter()
                .filter(|article| {
                    article.scope == NewsScope::Company
                        && article.relevance >= RELEVANT
                        && period.covers(&article.created_at)
                })
                .map(|article| article.relevance)
                .collect();
            Coverage {
                ticker: holding.ticker.clone(),
                stories: relevant.len(),
                top_relevance: relevant.iter().max().copied(),
            }
        })
        .collect()
}
