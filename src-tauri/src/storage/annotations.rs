//! Edits to transactions, saved apart from provider data: category, review, tags, splits,
//! refund links, and whether the money is someone else's.
use super::*;

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Annotation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reviewed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) benefit_confirmed: Option<bool>,
    /// Free-form labels. In a change, an empty list clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tags: Option<Vec<String>>,
    /// Parts that divide the amount between categories. In a change, an empty list clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) splits: Option<Vec<Split>>,
    /// The purchase this refund repays. In a change, an empty id clears the link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) refund_of: Option<String>,
    /// Someone else's money: a purchase made for them, or what they paid you back. In a
    /// change, false clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) other: Option<bool>,
}

/// One part of a split transaction. Parts carry the transaction's sign and sum to its amount.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Split {
    pub category: String,
    pub amount: f64,
}

const MAX_TAGS: usize = 12;

const MAX_SPLITS: usize = 12;

/// Trims, drops blanks and repeats, and bounds a transaction's tags.
fn normalize_tags(tags: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::<String>::new();
    for tag in tags {
        let tag = tag.trim().to_owned();
        if tag.chars().count() > 40 {
            return Err("A tag is too long".into());
        }
        if !tag.is_empty()
            && !normalized
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&tag))
        {
            normalized.push(tag);
        }
    }
    if normalized.len() > MAX_TAGS {
        return Err("Too many tags".into());
    }
    Ok(normalized)
}

/// Whether parts are a valid division of `amount`: two or more named parts, each with the
/// amount's sign, summing to it exactly in cents.
pub fn splits_divide(splits: &[Split], amount: f64) -> bool {
    let cents = |value: f64| crate::numeric::minor_units(value);
    (2..=MAX_SPLITS).contains(&splits.len())
        && splits.iter().all(|split| {
            let category = split.category.trim();
            !category.is_empty()
                && category.len() <= 100
                && split.amount.is_finite()
                && cents(split.amount) != 0
                && (split.amount < 0.0) == (amount < 0.0)
        })
        && splits.iter().map(|split| cents(split.amount)).sum::<i64>() == cents(amount)
}

impl Storage {
    pub fn save_annotations(
        &mut self,
        changes: BTreeMap<String, Annotation>,
        importing: bool,
    ) -> Result<BTreeMap<String, Annotation>, String> {
        if self.recovery.is_some() && importing {
            return Ok(self.data.annotations.clone());
        }
        self.ensure_writable()?;
        let mut next = self.data.annotations.clone();
        for (id, change) in changes {
            if id.is_empty()
                || id.len() > 200
                || change.category.as_ref().is_some_and(|value| {
                    value.len() > 100 || (importing && value.trim().is_empty())
                })
            {
                return Err("Invalid transaction annotation".into());
            }
            let entry = next.entry(id.clone()).or_default();
            match change.category {
                // An empty category clears a manual override so rules and the provider apply.
                Some(category) if category.trim().is_empty() => entry.category = None,
                Some(category) if !importing || entry.category.is_none() => {
                    entry.category = Some(category.trim().to_owned())
                }
                _ => {}
            }
            if change.reviewed.is_some() && (!importing || entry.reviewed.is_none()) {
                entry.reviewed = change.reviewed;
            }
            if change.benefit_confirmed.is_some()
                && (!importing || entry.benefit_confirmed.is_none())
            {
                entry.benefit_confirmed = change.benefit_confirmed;
            }
            // Tags, splits, refund links, and whose money it is are set in the app, never imported.
            if importing {
                continue;
            }
            if let Some(tags) = change.tags {
                let tags = normalize_tags(tags)?;
                entry.tags = (!tags.is_empty()).then_some(tags);
            }
            if let Some(splits) = change.splits {
                if splits.is_empty() {
                    entry.splits = None;
                } else {
                    let amount = self
                        .transaction_amount(&id)
                        .ok_or("That transaction is no longer available")?;
                    if !splits_divide(&splits, amount) {
                        return Err("Split parts must add up to the transaction amount".into());
                    }
                    entry.splits = Some(
                        splits
                            .into_iter()
                            .map(|split| Split {
                                category: split.category.trim().to_owned(),
                                amount: split.amount,
                            })
                            .collect(),
                    );
                }
            }
            if let Some(purchase) = change.refund_of {
                if purchase.is_empty() {
                    entry.refund_of = None;
                } else {
                    let refund = self.transaction_amount(&id);
                    let repaid = self.transaction_amount(&purchase);
                    if purchase == id
                        || purchase.len() > 200
                        || !refund
                            .zip(repaid)
                            .is_some_and(|(refund, repaid)| refund > 0.0 && repaid < 0.0)
                    {
                        return Err("A refund links a credit to an earlier purchase".into());
                    }
                    entry.refund_of = Some(purchase);
                }
            }
            if let Some(other) = change.other {
                entry.other = other.then_some(true);
            }
        }
        next.retain(|_, annotation| annotation != &Annotation::default());
        if next != self.data.annotations {
            self.backup_before_edit()?;
            self.database.replace_annotations(&next)?;
            self.data.annotations = next;
        }
        Ok(self.data.annotations.clone())
    }

    fn transaction_amount(&self, id: &str) -> Option<f64> {
        self.data
            .snapshot
            .transactions
            .iter()
            .find(|transaction| transaction.id == id)
            .map(|transaction| transaction.amount)
    }
}
