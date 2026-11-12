//! What each transaction means: its kind (income, spending, a transfer, someone else's money),
//! its category, refund links, and transfer pairs. Assigned on every read after saved edits;
//! the committed snapshot keeps the provider's categories, so a policy change applies to all
//! history.
//!
//! A category is the user's, else a suggested one, else the provider's, and a posted
//! transaction takes the edits of the pending one it replaced. Plaid's detailed code decides a
//! provider category's meaning; the name and text policy covers refunds, unknown codes, and
//! categories chosen by hand or rule. A positive issuer statement credit is always a credit,
//! since Plaid files those under the benefit's spending category, a transfer, or a fee.
use crate::{
    finance_contract::Transaction, financial_engine::categories::CodeKind, storage::Annotation,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    Transfer,
    Interest,
    Dividend,
    Income,
    Refund,
    Fee,
    Payment,
    Cash,
    Initial,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "TransactionKind")]
pub enum Kind {
    Income,
    Dividend,
    Interest,
    Fee,
    Tax,
    Reimbursement,
    Transfer,
    Expense,
    /// Someone else's money: a purchase made for them, or what they paid you back. Neither is
    /// your spending or your income.
    Owed,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub mark: Mark,
    pub kind: Kind,
    pub spending: bool,
    #[serde(default)]
    pub credit: bool,
    pub zelle: bool,
    /// A purchase made for someone else that would otherwise be spending. `spending` leaves it
    /// out; Spending counts it only when it shows someone else's spending, or everyone's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub others_spending: bool,
}
fn contains(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}
/// The longest gap between two legs of one transfer, covering ACH across a weekend.
pub(crate) const TRANSFER_SETTLEMENT_DAYS: i64 = 5;

/// Money sent to other people through payment apps. It is spending; money received through
/// one keeps the provider's meaning, so a friend paying you back is not income. A manual or
/// rule category still wins.
pub const PEOPLE_CATEGORY: &str = "Payments To People";

fn mentions_zelle(t: &Transaction) -> bool {
    word(
        &format!(
            "{} {}",
            t.merchant.to_lowercase(),
            t.description.as_deref().unwrap_or("").to_lowercase()
        ),
        &["zelle"],
    )
}

/// Whether a provider-categorized outflow is a Zelle payment Plaid filed as a transfer.
/// Payment-app transfers Plaid recognizes already map to the people category.
fn payment_to_person(t: &Transaction) -> bool {
    t.amount < 0.0
        && ["transfer out", "transfers"].contains(&t.category.to_lowercase().as_str())
        && mentions_zelle(t)
}

fn word(text: &str, terms: &[&str]) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|part| terms.contains(&part))
}
/// What a transaction is. `cards` are the card products whose issuer statement credits are
/// known, since those credits are told apart by their descriptors.
pub fn classify(t: &Transaction, cards: &crate::insights::benefits::Catalog) -> Classification {
    if t.amount > 0.0 && cards.is_issuer_credit(&t.descriptors()) {
        // A card statement credit repays benefit spending, whatever Plaid's category says.
        return Classification {
            mark: Mark::Refund,
            kind: Kind::Reimbursement,
            spending: false,
            credit: true,
            zelle: false,
            others_spending: false,
        };
    }
    classify_by_code(t).unwrap_or_else(|| classify_by_name(t))
}

/// Plaid's detailed code decides the meaning of a provider-categorized transaction. Refunds
/// and credits in spending categories, unknown codes, and manual or rule categories (whose
/// detail is cleared before classification) use the name and text policy below.
fn classify_by_code(t: &Transaction) -> Option<Classification> {
    let code = t.category_detail.as_deref()?;
    let kind = crate::financial_engine::categories::kind_for(code)?;
    let zelle = mentions_zelle(t);
    let outflow = t.amount < 0.0;
    let to_person = t.category == PEOPLE_CATEGORY;
    let (mark, kind, spending) = match kind {
        _ if to_person && outflow => (Mark::Transfer, Kind::Expense, true),
        CodeKind::Income => (Mark::Income, Kind::Income, false),
        CodeKind::Interest => (Mark::Interest, Kind::Interest, false),
        CodeKind::Dividend => (Mark::Dividend, Kind::Dividend, false),
        CodeKind::Transfer if code == "TRANSFER_OUT_WITHDRAWAL" => {
            (Mark::Cash, Kind::Transfer, false)
        }
        CodeKind::Transfer => (Mark::Transfer, Kind::Transfer, false),
        CodeKind::Payment => (Mark::Payment, Kind::Transfer, false),
        CodeKind::Fee if outflow => (Mark::Fee, Kind::Fee, true),
        CodeKind::Tax if outflow => (Mark::Initial, Kind::Tax, true),
        CodeKind::Expense if outflow => (Mark::Initial, Kind::Expense, true),
        _ => return None,
    };
    Some(Classification {
        mark,
        kind,
        spending,
        credit: false,
        zelle,
        others_spending: false,
    })
}

fn classify_by_name(t: &Transaction) -> Classification {
    let category = t.category.to_lowercase();
    let merchant = t.merchant.to_lowercase();
    let text = format!("{category} {merchant}");
    let zelle = mentions_zelle(t);
    let to_person = category == PEOPLE_CATEGORY.to_lowercase();
    let spending =
        t.amount < 0.0 && (to_person || !contains(&text, &["income", "transfer", "payment"]));
    // Plaid's v2 taxonomy reports loan proceeds separately; they are borrowed money moving
    // between accounts, never income or spending.
    let loan_disbursement = category == "loan disbursements";
    let mark =
        if category == "transfer" || category == "transfers" || loan_disbursement || to_person {
            Mark::Transfer
        } else if category == "reimbursement" {
            Mark::Refund
        } else if text.contains("interest") || word(&text, &["apy"]) {
            Mark::Interest
        } else if contains(
            &text,
            &["dividend", "capital gain", "investment distribution"],
        ) {
            Mark::Dividend
        } else if category == "income" {
            Mark::Income
        } else if contains(
            &text,
            &[
                "refund",
                "reimbursement",
                "cashback",
                "cash back",
                "reversal",
                "credit adjustment",
            ],
        ) {
            Mark::Refund
        } else if word(&text, &["fee", "fees"]) || contains(&text, &["overdraft", "service charge"])
        {
            Mark::Fee
        } else if text.contains("cash withdrawal") || word(&text, &["atm"]) {
            Mark::Cash
        } else if contains(
            &text,
            &[
                "credit card payment",
                "loan payment",
                "autopay",
                "bill payment",
            ],
        ) {
            Mark::Payment
        } else if text.contains("transfer") || word(&text, &["wire", "ach"]) {
            Mark::Transfer
        } else if contains(
            &text,
            &["income", "paycheck", "salary", "payroll", "direct deposit"],
        ) {
            Mark::Income
        } else {
            Mark::Initial
        };
    let category = category.trim();
    let detail = format!(
        "{text} {} {}",
        t.category_detail.as_deref().unwrap_or(""),
        t.description.as_deref().unwrap_or("")
    )
    .to_lowercase();
    let kind = if to_person && spending {
        Kind::Expense
    } else if category == "reimbursement" {
        Kind::Reimbursement
    } else if category == "income" {
        match mark {
            Mark::Interest => Kind::Interest,
            Mark::Dividend => Kind::Dividend,
            _ => Kind::Income,
        }
    } else if category == "transfer" || category == "transfers" || loan_disbursement {
        Kind::Transfer
    } else if contains(&detail, &["reimbursement", "shared expense repayment"]) {
        Kind::Reimbursement
    } else if [
        "fee",
        "fees",
        "bank fee",
        "bank fees",
        "investment fee",
        "investment fees",
    ]
    .contains(&category)
    {
        Kind::Fee
    } else if word(&detail, &["tax", "taxes"]) || detail.contains("withholding") {
        Kind::Tax
    } else {
        match mark {
            Mark::Income => Kind::Income,
            Mark::Dividend => Kind::Dividend,
            Mark::Interest => Kind::Interest,
            Mark::Fee => Kind::Fee,
            Mark::Transfer | Mark::Payment => Kind::Transfer,
            _ if spending => Kind::Expense,
            _ => Kind::Other,
        }
    };
    let credit = t.amount > 0.0 && (mark == Mark::Refund || word(&text, &["credit"]));
    Classification {
        credit,
        mark,
        kind,
        spending,
        zelle,
        others_spending: false,
    }
}

/// Where a transaction's category came from: chosen by hand, the provider's, or suggested by
/// Brief from your earlier choices (`suggest_categories`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum CategorySource {
    Manual,
    Provider,
    Suggested,
}

/// The least confidence a suggested category is applied at; below it the provider's stands.
pub const SUGGEST_CONFIDENCE: f64 = 0.65;
/// The least cosine similarity between two merchants' names for one's choices to count for the
/// other.
pub const NEIGHBOUR_SIMILARITY: f32 = 0.8;
/// How many similar merchants vote at most.
const NEIGHBOURS: usize = 5;
/// How many earlier choices a suggestion names.
const SUGGESTION_EVIDENCE: usize = 12;

/// What a suggestion followed: your choices for the same merchant, or, when you never chose
/// one for it, your choices for merchants whose names mean nearly the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionBasis {
    Merchant,
    Similar,
}

/// Why Brief chose a transaction's category for you.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CategorySuggestion {
    /// See `suggest_categories` for the formula.
    pub confidence: f64,
    pub basis: SuggestionBasis,
    /// Your earlier transactions it follows, newest first.
    pub from: Vec<String>,
}

/// The merchant identity choices are learned under: Plaid's entity when known, otherwise the
/// name's letters and digits, in any script. A name with none has no identity to learn under.
fn merchant_key(t: &Transaction) -> Option<String> {
    t.merchant_entity_id.clone().or_else(|| {
        let key = t
            .merchant
            .chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>();
        (!key.is_empty()).then_some(key)
    })
}

/// Your categorizations of one merchant: each transaction you categorized votes for its
/// category, and each suggestion you rejected is a vote that agrees with none.
#[derive(Clone, Default)]
struct Votes {
    /// Category → the transactions you gave it, with their dates.
    chosen: BTreeMap<String, Vec<(String, String)>>,
    rejected: usize,
}

impl Votes {
    fn total(&self) -> usize {
        self.chosen.values().map(Vec::len).sum::<usize>() + self.rejected
    }

    fn add(&mut self, other: &Votes) {
        for (category, ids) in &other.chosen {
            self.chosen
                .entry(category.clone())
                .or_default()
                .extend(ids.iter().cloned());
        }
        self.rejected += other.rejected;
    }

    /// The category most of the votes chose, other than `excluded`, with its confidence and
    /// the choices behind it. Confidence is the share of votes that agree, discounted for few
    /// of them: agree / (votes + 1), so one choice is 0.5, two that agree 0.67, and three of
    /// four 0.6. Ties go to the alphabetically first category, so a history always reads alike.
    fn best(&self, excluded: Option<&str>) -> Option<(String, f64, Vec<String>)> {
        let total = self.total();
        let (category, ids) = self
            .chosen
            .iter()
            .filter(|(category, _)| Some(category.as_str()) != excluded)
            .max_by(|left, right| {
                left.1
                    .len()
                    .cmp(&right.1.len())
                    .then_with(|| right.0.cmp(left.0))
            })?;
        let mut from = ids.clone();
        from.sort_by(|left, right| right.1.cmp(&left.1).then(right.0.cmp(&left.0)));
        Some((
            category.clone(),
            ids.len() as f64 / (total as f64 + 1.0),
            from.into_iter()
                .take(SUGGESTION_EVIDENCE)
                .map(|(id, _)| id)
                .collect(),
        ))
    }
}

/// Whether a transaction may take a suggested category: an ordinary purchase whose category is
/// still the provider's, not split, not a refund, not a move between your accounts, and not a
/// payment to a person.
fn suggestible(t: &Transaction) -> bool {
    t.category_source == Some(CategorySource::Provider)
        && t.amount < 0.0
        && t.splits.is_empty()
        && t.refund_of.is_none()
        && t.transfer_pair_id.is_none()
        && t.category != PEOPLE_CATEGORY
        && t.classification
            .as_ref()
            .is_some_and(|classification| classification.kind == Kind::Expense)
}

/// Suggests categories from your own choices, after `apply`. For each purchase whose category
/// is still the provider's, the votes are your categorizations of the same merchant; only when
/// there are none, those of up to `NEIGHBOURS` merchants whose names' on-device embeddings
/// (`embedder`) lie within `NEIGHBOUR_SIMILARITY` of its own. The leading category is applied
/// when its confidence (`Votes::best`) reaches `SUGGEST_CONFIDENCE` and it differs from the
/// provider's, and never when you rejected it for that transaction (`rejections`). Each change
/// you make changes the votes, so the next read follows it. The result depends only on the
/// history, your choices, and the embedding of each name, which the cache keeps.
pub fn suggest_categories(
    transactions: &mut [Transaction],
    rejections: &BTreeMap<String, String>,
    embedder: Option<&dyn crate::search::embeddings::Embedder>,
    cards: &crate::insights::benefits::Catalog,
) {
    let mut votes = BTreeMap::<String, Votes>::new();
    // Each voting merchant is compared by the name on its most recently categorized purchase.
    let mut names = BTreeMap::<String, (String, String)>::new();
    for t in transactions.iter() {
        let Some(key) = merchant_key(t) else {
            continue;
        };
        if t.category_source == Some(CategorySource::Manual) && t.splits.is_empty() {
            votes
                .entry(key.clone())
                .or_default()
                .chosen
                .entry(t.category.clone())
                .or_default()
                .push((t.id.clone(), t.date.clone()));
            let name = names
                .entry(key)
                .or_insert_with(|| (t.date.clone(), t.merchant.clone()));
            if t.date > name.0 {
                *name = (t.date.clone(), t.merchant.clone());
            }
        } else if rejections.contains_key(&t.id) {
            votes.entry(key).or_default().rejected += 1;
        }
    }
    let candidates = transactions
        .iter()
        .enumerate()
        .filter(|(_, t)| suggestible(t))
        .filter_map(|(index, t)| Some((index, merchant_key(t)?)))
        .collect::<Vec<_>>();
    // Merchants you never categorized learn from similar names, embedded once per name.
    let unknown = candidates
        .iter()
        .filter(|(_, key)| votes.get(key).is_none_or(|votes| votes.total() == 0))
        .map(|(index, _)| transactions[*index].merchant.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let neighbours = match embedder {
        Some(embedder) if !unknown.is_empty() && !names.is_empty() => {
            similar_merchants(embedder, &unknown, &names)
        }
        _ => BTreeMap::new(),
    };
    for (index, key) in candidates {
        let t = &transactions[index];
        let rejected = rejections.get(&t.id).map(String::as_str);
        let (pool, basis) = match votes.get(&key).filter(|votes| votes.total() > 0) {
            Some(own) => (std::borrow::Cow::Borrowed(own), SuggestionBasis::Merchant),
            None => {
                let mut pooled = Votes::default();
                for similar in neighbours.get(&t.merchant).into_iter().flatten() {
                    if let Some(found) = votes.get(similar) {
                        pooled.add(found);
                    }
                }
                (std::borrow::Cow::Owned(pooled), SuggestionBasis::Similar)
            }
        };
        let Some((category, confidence, from)) = pool.best(rejected) else {
            continue;
        };
        if confidence < SUGGEST_CONFIDENCE || category == t.category {
            continue;
        }
        let t = &mut transactions[index];
        let provider = std::mem::replace(&mut t.category, category);
        // A chosen category is classified by its name, as a manual one is.
        let detail = t.category_detail.take();
        let classification = classify(t, cards);
        t.category_detail = detail;
        // A suggestion only moves a purchase between spending categories. One that made it a
        // transfer or income would need pairing and refunds read again, so it isn't made.
        if classification.kind != Kind::Expense {
            t.category = provider;
            continue;
        }
        t.provider_category.get_or_insert(provider);
        t.category_source = Some(CategorySource::Suggested);
        t.subcategory = None;
        t.category_suggestion = Some(CategorySuggestion {
            confidence,
            basis,
            from,
        });
        t.classification = Some(classification);
    }
}

/// For each unknown merchant name, the voting merchants (by key) whose names are within
/// `NEIGHBOUR_SIMILARITY`, most similar first. Names the embedder can't embed have none.
fn similar_merchants(
    embedder: &dyn crate::search::embeddings::Embedder,
    unknown: &std::collections::BTreeSet<String>,
    names: &BTreeMap<String, (String, String)>,
) -> BTreeMap<String, Vec<String>> {
    let known = names
        .iter()
        .map(|(key, (_, name))| (key.clone(), name.clone()))
        .collect::<Vec<_>>();
    let texts = unknown
        .iter()
        .cloned()
        .chain(known.iter().map(|(_, name)| name.clone()))
        .collect::<Vec<_>>();
    let Some(vectors) = embedder.embed(&texts) else {
        return BTreeMap::new();
    };
    if vectors.len() != texts.len() {
        return BTreeMap::new();
    }
    let unit = |vector: &[f32]| {
        let length = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        (length > 0.0 && length.is_finite()).then(|| {
            vector
                .iter()
                .map(|value| value / length)
                .collect::<Vec<_>>()
        })
    };
    let (unknown_vectors, known_vectors) = vectors.split_at(unknown.len());
    let known_units = known_vectors.iter().map(|v| unit(v)).collect::<Vec<_>>();
    unknown
        .iter()
        .zip(unknown_vectors)
        .filter_map(|(name, vector)| {
            let own = unit(vector)?;
            let mut close = known
                .iter()
                .zip(&known_units)
                .filter_map(|((key, _), other)| {
                    let other = other.as_ref()?;
                    let similarity = own.iter().zip(other).map(|(a, b)| a * b).sum::<f32>();
                    (similarity >= NEIGHBOUR_SIMILARITY).then(|| (similarity, key.clone()))
                })
                .collect::<Vec<_>>();
            close.sort_by(|left, right| right.0.total_cmp(&left.0).then(left.1.cmp(&right.1)));
            close.truncate(NEIGHBOURS);
            (!close.is_empty()).then(|| {
                (
                    name.clone(),
                    close.into_iter().map(|(_, key)| key).collect(),
                )
            })
        })
        .collect()
}

/// Where a transaction's people came from: chosen by hand (possibly nobody), or matched by a
/// person's match name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PeopleSource {
    Manual,
    Matched,
}

/// Applies saved edits, then classifies every transaction: on each read, and without edits
/// when a refresh projects. A legacy row that carries only an account name
/// takes that account's ID when exactly one account has the name.
pub fn apply<'a>(
    transactions: &mut [Transaction],
    accounts: impl IntoIterator<Item = (&'a str, &'a str)>,
    annotations: &BTreeMap<String, Annotation>,
    cards: &crate::insights::benefits::Catalog,
) {
    let mut names = BTreeMap::<&str, Option<&str>>::new();
    for (id, name) in accounts {
        if id != "all" {
            names
                .entry(name)
                .and_modify(|id| *id = None)
                .or_insert(Some(id));
        }
    }
    let mut refunds = Vec::<(usize, String)>::new();
    for (index, t) in transactions.iter_mut().enumerate() {
        if t.account_id.is_none() {
            if let Some(Some(id)) = names.get(t.account.as_str()) {
                t.account_id = Some((*id).to_owned());
            }
        }
        // A posted transaction keeps edits made while it was pending.
        let annotation = annotations.get(&t.id).or_else(|| {
            t.pending_transaction_id
                .as_ref()
                .and_then(|pending| annotations.get(pending))
        });
        let (category, source) = match annotation.and_then(|a| a.category.as_ref()) {
            Some(category) => (category.clone(), CategorySource::Manual),
            None if payment_to_person(t) => (PEOPLE_CATEGORY.to_owned(), CategorySource::Provider),
            None => (t.category.clone(), CategorySource::Provider),
        };
        let provider_category = std::mem::replace(&mut t.category, category);
        let edited = source != CategorySource::Provider;
        t.provider_category = (t.category != provider_category).then_some(provider_category);
        t.category_source = Some(source);
        if edited {
            // Plaid's label describes Plaid's category, not one chosen by hand.
            t.subcategory = None;
        } else if t.provider_category.is_some() {
            // Only a Zelle payment to a person changes a provider category.
            t.subcategory = Some("Zelle".into());
        }
        t.reviewed = annotation
            .and_then(|a| a.reviewed)
            .filter(|reviewed| *reviewed);
        if let Some(confirmed) = annotation.and_then(|a| a.benefit_confirmed) {
            t.benefit_confirmed = Some(confirmed);
        }
        t.benefit_id = annotation.and_then(|a| a.benefit_id.clone());
        // Decided again from Plaid's own name on every read, so the result follows the records.
        if let Some(original) = t.provider_merchant.take() {
            t.merchant = original;
        }
        if let Some(name) = crate::merchants::corrected_name(t) {
            t.provider_merchant = Some(std::mem::replace(&mut t.merchant, name));
            // Plaid's logo and site belong to the merchant it named.
            t.logo_url = None;
            t.logo_name = None;
            t.website = None;
        }
        t.presence = crate::merchants::presence(t);
        // A remote purchase, or one through a processor that prints its own office (a vending
        // machine's), says nothing about where the cardholder was.
        t.place = (t.presence != Some(crate::merchants::Presence::Remote)
            && !crate::merchants::bills_elsewhere(t))
        .then(|| crate::places::resolve(t))
        .flatten();
        t.tags = annotation.and_then(|a| a.tags.clone()).unwrap_or_default();
        t.note = annotation.and_then(|a| a.note.clone());
        // People chosen by hand; matching by name fills the rest once settings are known.
        let people = annotation.and_then(|a| a.people.as_ref());
        t.people = people.cloned().unwrap_or_default();
        t.people_source = people.map(|_| PeopleSource::Manual);
        // A split made for another amount (a pending charge that posted differently) no
        // longer divides this one, so the transaction stays whole.
        t.splits = annotation
            .and_then(|a| a.splits.as_ref())
            .filter(|splits| crate::storage::splits_divide(splits, t.amount))
            .cloned()
            .unwrap_or_default();
        t.refund_of = None;
        // Your link first; otherwise the purchase Wallet's own record repays.
        let linked = annotation
            .and_then(|a| a.refund_of.as_ref())
            .or_else(|| t.wallet.as_ref().and_then(|w| w.refund_of.as_ref()));
        if let Some(purchase) = linked {
            refunds.push((index, purchase.clone()));
        }
        // A category chosen by hand or rule is classified by its name, not Plaid's code.
        let mut classification = if edited {
            let detail = t.category_detail.take();
            let classification = classify(t, cards);
            t.category_detail = detail;
            classification
        } else {
            classify(t, cards)
        };
        t.other = annotation.and_then(|a| a.other) == Some(true);
        if t.other {
            owe(&mut classification, t.amount);
        }
        t.classification = Some(classification);
        t.benefit_credit = (t.amount > 0.0).then(|| cards.credit_identity(t)).flatten();
    }
    link_refunds(transactions, refunds);
    pair_transfers(transactions);
}

/// A linked refund repays its purchase: it takes that purchase's category, unless one was
/// chosen by hand, and reduces spending instead of counting as income or a transfer. Links
/// are `(refund's index, purchase id)`, from your edits or an Explained rule.
pub(crate) fn link_refunds(transactions: &mut [Transaction], refunds: Vec<(usize, String)>) {
    for (index, purchase_id) in refunds {
        let Some((category, purchase_other)) = transactions
            .iter()
            .find(|candidate| candidate.id == purchase_id)
            .filter(|purchase| purchase.amount < 0.0)
            .map(|purchase| (purchase.category.clone(), purchase.other))
        else {
            continue;
        };
        let refund = &mut transactions[index];
        if refund.amount <= 0.0 {
            continue;
        }
        refund.refund_of = Some(purchase_id);
        // A refund marked someone else's is already out of your spending.
        if refund.other {
            continue;
        }
        if refund.category_source != Some(CategorySource::Manual) {
            if refund.provider_category.is_none() && refund.category != category {
                refund.provider_category = Some(refund.category.clone());
            }
            refund.category = category;
            refund.subcategory = None;
        }
        if let Some(classification) = refund.classification.as_mut() {
            classification.kind = Kind::Reimbursement;
            classification.mark = Mark::Refund;
            classification.spending = false;
            // A refund of a purchase made for someone else is theirs too, not your spending.
            if purchase_other {
                classification.kind = Kind::Owed;
            }
        }
        refund.other |= purchase_other;
    }
}

/// Pairs the two legs of each move between two of your accounts, so every reader agrees which
/// transactions are one movement of money: net worth counts it once while its legs post on
/// different days, Data health knows a card received its payment, and the renderer links the
/// legs. A leg's other half is an equal and opposite posted leg in another account within the
/// settlement window, the nearest by date, and each leg pairs once.
///
/// The provider sometimes files one leg as something else: an instant payment to yourself can
/// arrive as contractor income, and a move out of a brokerage's cash can leave as spending or
/// "Other". Such a leg pairs only with a leg that is already a transfer, after transfers have
/// paired with each other, and becomes a transfer itself, unless its category was chosen by
/// hand or by rule. Cash withdrawals, money through a payment app (which is with a person), and
/// someone else's money never pair.
fn pair_transfers(transactions: &mut [Transaction]) {
    struct Leg {
        index: usize,
        date: chrono::NaiveDate,
        account: String,
        cents: i64,
        transfer: bool,
    }
    for t in transactions.iter_mut() {
        t.transfer_pair_id = None;
    }
    let mut legs = transactions
        .iter()
        .enumerate()
        .filter_map(|(index, t)| {
            let classification = t.classification.as_ref()?;
            let kind = &classification.kind;
            let cents = crate::numeric::minor_units(t.amount);
            let payment_app = classification.zelle
                || t.category == PEOPLE_CATEGORY
                || t.category_detail
                    .as_deref()
                    .is_some_and(|code| code.ends_with("_FROM_APPS"));
            let transfer = *kind == Kind::Transfer && classification.mark != Mark::Cash;
            let filed = t.category_source == Some(CategorySource::Provider)
                && match kind {
                    Kind::Income => cents > 0,
                    Kind::Expense => cents < 0,
                    Kind::Other => true,
                    _ => false,
                };
            let date = chrono::NaiveDate::parse_from_str(t.posted_date(), "%Y-%m-%d").ok()?;
            let account = t.account_id.clone()?;
            ((transfer || filed) && !payment_app && !t.pending && !t.other && cents != 0).then_some(
                Leg {
                    index,
                    date,
                    account,
                    cents,
                    transfer,
                },
            )
        })
        .collect::<Vec<_>>();
    // Provider order isn't stable, so legs pair in date order and ties fall to the id.
    legs.sort_by(|left, right| {
        (left.date, &transactions[left.index].id).cmp(&(right.date, &transactions[right.index].id))
    });
    let mut paired = vec![false; legs.len()];
    let mut pairs = Vec::new();
    for mixed in [false, true] {
        for out in 0..legs.len() {
            if paired[out] || legs[out].cents > 0 {
                continue;
            }
            let Some(into) = (0..legs.len())
                .filter(|&into| {
                    let (sent, arrived) = (&legs[out], &legs[into]);
                    !paired[into]
                        && arrived.cents == -sent.cents
                        && arrived.account != sent.account
                        && (arrived.date - sent.date).num_days().abs() <= TRANSFER_SETTLEMENT_DAYS
                        && if mixed {
                            sent.transfer != arrived.transfer
                        } else {
                            sent.transfer && arrived.transfer
                        }
                })
                .min_by_key(|&into| (legs[into].date - legs[out].date).num_days().abs())
            else {
                continue;
            };
            paired[out] = true;
            paired[into] = true;
            pairs.push((out, into));
        }
    }
    for (out, into) in pairs {
        let (sent, arrived) = (legs[out].index, legs[into].index);
        transactions[sent].transfer_pair_id = Some(transactions[arrived].id.clone());
        transactions[arrived].transfer_pair_id = Some(transactions[sent].id.clone());
        let (filed, transfer) = match (legs[out].transfer, legs[into].transfer) {
            (true, false) => (arrived, sent),
            (false, true) => (sent, arrived),
            _ => continue,
        };
        let category = transactions[transfer].category.clone();
        let leg = &mut transactions[filed];
        if leg.provider_category.is_none() {
            leg.provider_category = Some(leg.category.clone());
        }
        leg.category = category;
        leg.subcategory = None;
        if let Some(classification) = leg.classification.as_mut() {
            classification.kind = Kind::Transfer;
            classification.mark = Mark::Transfer;
            classification.spending = false;
        }
    }
}

/// Someone else's money: a purchase made for them, or what they paid you back.
pub(crate) fn owe(classification: &mut Classification, amount: f64) {
    classification.kind = Kind::Owed;
    classification.others_spending |= classification.spending;
    classification.spending = false;
    classification.credit = false;
    if amount > 0.0 {
        classification.mark = Mark::Transfer;
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod suggestions {
    use super::*;
    use crate::insights::benefits::Catalog;

    fn suggested(merchants: &[(&str, &str)], chosen: &[(&str, &str)]) -> Vec<Transaction> {
        let mut transactions = merchants
            .iter()
            .map(|(id, merchant)| purchase(id, merchant))
            .collect::<Vec<_>>();
        let annotations = chosen
            .iter()
            .map(|(id, category)| {
                (
                    (*id).to_owned(),
                    Annotation {
                        category: Some((*category).into()),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let cards = Catalog::default();
        apply(&mut transactions, [], &annotations, &cards);
        suggest_categories(&mut transactions, &BTreeMap::new(), None, &cards);
        transactions
    }

    #[test]
    fn a_suggestion_never_turns_a_purchase_into_a_transfer() {
        let transactions = suggested(
            &[
                ("a", "Example Pay"),
                ("b", "Example Pay"),
                ("c", "Example Pay"),
            ],
            &[("a", "Transfer"), ("b", "Transfer")],
        );
        let third = transactions.iter().find(|t| t.id == "c").unwrap();
        assert_eq!(third.category, "Shopping");
        assert_eq!(third.kind(), Some(Kind::Expense));
    }

    fn purchase(id: &str, merchant: &str) -> Transaction {
        serde_json::from_value(serde_json::json!({
            "id": id, "merchant": merchant, "category": "Shopping", "amount": -20.0,
            "account": "Example", "accountId": "plaid:card", "date": "2026-09-01",
            "pending": false,
        }))
        .unwrap()
    }

    #[test]
    fn a_name_without_letters_learns_nothing_and_other_scripts_keep_their_own() {
        let mut transactions = vec![
            purchase("cafe-1", "カフェ"),
            purchase("cafe-2", "カフェ"),
            purchase("cafe-3", "カフェ"),
            purchase("noodles", "ラーメン"),
            purchase("blank", ""),
            purchase("dots", "..."),
        ];
        let annotations = ["cafe-1", "cafe-2"]
            .into_iter()
            .map(|id| {
                (
                    id.to_owned(),
                    Annotation {
                        category: Some("Dining".into()),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let cards = Catalog::default();
        apply(&mut transactions, [], &annotations, &cards);
        suggest_categories(&mut transactions, &BTreeMap::new(), None, &cards);
        let category = |id: &str| {
            transactions
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.category.as_str())
                .unwrap()
        };
        assert_eq!(category("cafe-3"), "Dining");
        for id in ["noodles", "blank", "dots"] {
            assert_eq!(category(id), "Shopping", "{id}");
        }
    }
}
