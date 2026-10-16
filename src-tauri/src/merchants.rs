//! Who a card purchase was with, and whether the cardholder was there, from every record of
//! it: the card issuer's export, the bank's descriptor, and Plaid's enrichment, trusted in that
//! order.
//!
//! Plaid's merchant name stands unless the bank's own records contradict it: another brand
//! when the issuer's export, Plaid's own location, or Plaid's low confidence backs the records;
//! another number ("Curry 36" is not "Curry 42"); or a name Plaid didn't recognize that the
//! records spell otherwise. A purchase is remote when its processor, its words, the issuer's
//! category, or Plaid's category say so; processors, brand names, and those words live in
//! `merchants/descriptors.json`.
use std::{collections::BTreeMap, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{finance_contract::Transaction, places};

/// Whether the cardholder was where the merchant is.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Presence {
    InPerson,
    /// Bought online, by subscription, or by phone: where the merchant bills from says nothing
    /// about where the cardholder was.
    Remote,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Processor {
    prefix: String,
    presence: Option<Presence>,
    /// Written without a `*` before the merchant ("LS CURRY 36").
    #[serde(default)]
    bare: bool,
    /// The issuer's address is the processor's, not the machine's or stand's.
    #[serde(default)]
    bills_elsewhere: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rules {
    processors: Vec<Processor>,
    /// Words a wallet or bank puts before the merchant, as normalized.
    leading: Vec<String>,
    /// A descriptor's leading words, as normalized, to the brand they abbreviate.
    aliases: BTreeMap<String, String>,
    remote_words: Vec<String>,
    /// Lowercase parts of an issuer's category.
    remote_categories: Vec<String>,
    /// Prefixes of Plaid's detailed category codes.
    remote_codes: Vec<String>,
}

static RULES: LazyLock<Rules> = LazyLock::new(|| {
    serde_json::from_str(include_str!("merchants/descriptors.json"))
        .expect("merchant descriptor rules are valid")
});

/// A processor or brand written before a `*` or `@`: "TST* EL JARDIN", "OPENAI *CHATGPT".
static PREFIXED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?i:aplpay\s+|apple pay\s+)?([A-Za-z0-9]{2,12}) ?[*@]").expect("valid pattern")
});

enum Prefix {
    Processor(&'static Processor),
    /// Not a processor: a brand selling a product remotely ("ANTHROPIC* CLAUDE").
    Brand,
}

fn prefix(text: &str) -> Option<Prefix> {
    if let Some(captures) = PREFIXED.captures(text) {
        let token = captures[1].to_ascii_uppercase();
        return Some(
            RULES
                .processors
                .iter()
                .find(|processor| processor.prefix == token)
                .map_or(Prefix::Brand, Prefix::Processor),
        );
    }
    let merchant = places::descriptor_merchant(text);
    let first = without_leading(merchant.split(' ').collect())
        .into_iter()
        .next()?;
    RULES
        .processors
        .iter()
        .find(|processor| processor.bare && processor.prefix == first)
        .map(Prefix::Processor)
}

fn descriptor(transaction: &Transaction) -> Option<&str> {
    [&transaction.original_description, &transaction.description]
        .into_iter()
        .flatten()
        .map(|text| text.trim())
        .find(|text| !text.is_empty())
}

fn issuer_name(transaction: &Transaction) -> Option<&str> {
    transaction.issuer.as_ref()?.name.as_deref()
}

/// The processors written before the merchant, in the descriptor or the issuer's fuller name.
fn processor(transaction: &Transaction) -> Option<Prefix> {
    [descriptor(transaction), issuer_name(transaction)]
        .into_iter()
        .flatten()
        .find_map(prefix)
}

/// A purchase, by Plaid's channel or because a card issuer's export lists it. Refunds and
/// statement credits keep the name and place their records give.
fn purchase(transaction: &Transaction) -> bool {
    transaction.amount < 0.0
        && (matches!(
            transaction.payment_channel.as_deref(),
            Some("in store" | "online")
        ) || transaction.issuer.is_some())
}

pub fn presence(transaction: &Transaction) -> Option<Presence> {
    if !purchase(transaction) {
        return None;
    }
    let descriptor = descriptor(transaction).unwrap_or_default();
    let text = format!(
        "{descriptor} {}",
        issuer_name(transaction).unwrap_or_default()
    );
    let words = places::normalize(&text);
    let words = words.split(' ').collect::<Vec<_>>();
    let processor = processor(transaction);
    let category = transaction
        .issuer
        .as_ref()
        .and_then(|issuer| issuer.category.as_deref())
        .unwrap_or_default()
        .to_lowercase();
    let remote = match &processor {
        Some(Prefix::Processor(processor)) => processor.presence == Some(Presence::Remote),
        Some(Prefix::Brand) => true,
        None => false,
    } || RULES
        .remote_words
        .iter()
        .any(|word| words.contains(&word.as_str()))
        || places::names_a_website(&text)
        || RULES
            .remote_categories
            .iter()
            .any(|remote| category.contains(remote.as_str()))
        || transaction.category_detail.as_deref().is_some_and(|code| {
            RULES
                .remote_codes
                .iter()
                .any(|remote| code.starts_with(remote.as_str()))
        });
    if remote {
        return Some(Presence::Remote);
    }
    let processor_in_person = matches!(
        processor,
        Some(Prefix::Processor(processor)) if processor.presence == Some(Presence::InPerson)
    );
    let address = transaction
        .issuer
        .as_ref()
        .is_some_and(|issuer| issuer.address.is_some());
    match transaction.payment_channel.as_deref() {
        Some("online") if !processor_in_person => Some(Presence::Remote),
        Some("in store") => Some(Presence::InPerson),
        _ if processor_in_person || address => Some(Presence::InPerson),
        _ => None,
    }
}

/// Whether the issuer's address is the processor's rather than where the purchase was made.
pub fn bills_elsewhere(transaction: &Transaction) -> bool {
    matches!(
        processor(transaction),
        Some(Prefix::Processor(processor)) if processor.bills_elsewhere
    )
}

/// The words without what a wallet or bank put before the merchant ("DEBIT CARD", "APLPAY").
fn without_leading(mut words: Vec<&str>) -> Vec<&str> {
    while let Some(leading) = RULES.leading.iter().find(|leading| {
        let leading = leading.split(' ').collect::<Vec<_>>();
        words.len() > leading.len() && words[..leading.len()] == leading[..]
    }) {
        words.drain(..leading.split(' ').count());
    }
    words
}

const GENERIC: [&str; 8] = ["STORE", "STORES", "INC", "LLC", "CO", "CORP", "THE", "AND"];

/// The words that name the merchant: without a wallet or processor prefix, store and reference
/// numbers, generic words, or a last letter left by truncation.
fn name_words(normalized: &str, drop_processor: bool) -> Vec<String> {
    let mut words = without_leading(
        normalized
            .split(' ')
            .filter(|word| !word.is_empty())
            .collect(),
    );
    if drop_processor && words.len() > 1 {
        words.remove(0);
    }
    words.retain(|word| {
        let digits = word.bytes().filter(u8::is_ascii_digit).count();
        let reference = digits >= 3 || (digits > 0 && digits < word.len() && word.len() >= 4);
        !reference && !GENERIC.contains(word)
    });
    if words.len() > 1 && words.last().is_some_and(|word| word.len() == 1) {
        words.pop();
    }
    words.into_iter().map(str::to_owned).collect()
}

/// The words the bank's own records name the merchant with: the issuer's fuller name, else the
/// descriptor's merchant part.
pub fn evidence(transaction: &Transaction) -> Vec<String> {
    let Some(descriptor) = descriptor(transaction) else {
        return Vec::new();
    };
    let (raw, normalized) = match issuer_name(transaction) {
        Some(name) => (name, places::normalize(&unquoted(name))),
        None => (
            descriptor,
            places::descriptor_merchant(&unquoted(descriptor)),
        ),
    };
    name_words(
        &normalized,
        matches!(prefix(raw), Some(Prefix::Processor(_))),
    )
}

/// Whether the descriptor numbers the store ("SAFEWAY #1196", "SPEEDWAY 4379"): one branch of
/// a chain, which a name alone can't find.
pub fn numbered_store(transaction: &Transaction) -> bool {
    descriptor(transaction).is_some_and(|descriptor| {
        descriptor.contains('#')
            || places::descriptor_merchant(descriptor)
                .split(' ')
                .any(|word| word.len() >= 3 && word.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

/// Without apostrophes, so "Macy's" and MACYS are one word.
fn unquoted(text: &str) -> String {
    text.replace(['\'', '’'], "")
}

/// A business name's words, compared like the records'.
pub fn words(name: &str) -> Vec<String> {
    name_words(&places::normalize(&unquoted(name)), false)
}

/// Whether a business's name is the one the records give, directly or by a known abbreviation:
/// the records' first two words (which may be truncated, or run together) start it, or it is
/// exactly their first words. "Pizza Bocca Lupo" is PIZZA BOCCA LUPO S, and "The Melt" is MELT
/// SANTA CLARA; "Pizza Hut", "Karthikeya Supermarket" for KARTHIKEYA KITCHEN, and "Sar" for
/// SARAVANAA BHA are not.
pub fn names(name: &[String], evidence: &[String]) -> bool {
    let spelled = |evidence: &[String]| {
        let start = evidence.iter().take(2).cloned().collect::<String>();
        let whole = name.concat();
        start.len() >= 3
            && (whole.starts_with(&start)
                || (1..=evidence.len()).any(|count| evidence[..count].concat() == whole))
    };
    spelled(evidence) || alias(evidence).is_some_and(|alias| spelled(&words(alias)))
}

/// Whether a business's name is exactly the records' first words, not a department or a
/// longer name that starts the same ("Safeway", not "Safeway Pharmacy").
pub fn names_exactly(name: &[String], evidence: &[String]) -> bool {
    let whole = name.concat();
    let exact =
        |evidence: &[String]| (1..=evidence.len()).any(|count| evidence[..count].concat() == whole);
    exact(evidence) || alias(evidence).is_some_and(|alias| exact(&words(alias)))
}

fn alias(words: &[String]) -> Option<&'static str> {
    RULES.aliases.iter().find_map(|(key, brand)| {
        let key = key.split(' ').collect::<Vec<_>>();
        words
            .iter()
            .map(String::as_str)
            .take(key.len())
            .eq(key.iter().copied())
            .then_some(brand.as_str())
    })
}

/// The first word names the brand: a truncated one is a prefix of the other, a name run
/// together ("SOMISOMI SOFT") still starts with it, and an operator's prefix ("DU CONOCO")
/// leaves it among the other's words. Short words ("EL JARDIN") compare as the whole name.
fn same_brand(left: &[String], right: &[String]) -> bool {
    let (Some(left_first), Some(right_first)) = (left.first(), right.first()) else {
        return false;
    };
    let prefix = |long: &str, short: &str| short.len() >= 3 && long.starts_with(short);
    let among = |first: &str, words: &[String]| words.iter().any(|word| prefix(word, first));
    among(left_first, right)
        || among(right_first, left)
        || prefix(&left.concat(), right_first)
        || prefix(&right.concat(), left_first)
        || same_name(left, right)
}

/// The names agree over the first ten letters both have.
fn same_name(left: &[String], right: &[String]) -> bool {
    let (left, right) = (left.concat(), right.concat());
    let length = left.len().min(right.len()).min(10);
    length >= 3 && left[..length] == right[..length]
}

fn numbers(words: &[String]) -> Vec<&str> {
    words
        .iter()
        .filter(|word| word.bytes().all(|byte| byte.is_ascii_digit()))
        .map(String::as_str)
        .collect()
}

/// Plaid's merchant, unless it's Plaid's own enrichment.
pub fn plaid_named(transaction: &Transaction) -> bool {
    transaction.merchant_entity_id.is_some()
        || transaction
            .counterparties
            .iter()
            .any(|counterparty| counterparty.r#type.as_deref() == Some("merchant"))
}

/// Plaid recognized the merchant, rather than cleaning up the descriptor.
pub fn plaid_sure(transaction: &Transaction) -> bool {
    transaction.counterparties.iter().any(|counterparty| {
        counterparty.r#type.as_deref() == Some("merchant")
            && matches!(
                counterparty.confidence.as_deref(),
                Some("VERY_HIGH" | "HIGH" | "MEDIUM")
            )
    })
}

/// The name the bank's own records give, when they contradict Plaid's.
pub fn corrected_name(transaction: &Transaction) -> Option<String> {
    if !purchase(transaction) || !plaid_named(transaction) {
        return None;
    }
    let descriptor = descriptor(transaction)?;
    let evidence = evidence(transaction);
    if evidence.is_empty() || (evidence.len() == 1 && evidence[0].len() > 12) {
        // A name run together into one long word doesn't read as a name.
        return None;
    }
    let sure = plaid_sure(transaction);
    let shown_processed =
        !sure && matches!(prefix(&transaction.merchant), Some(Prefix::Processor(_)));
    let shown = name_words(&places::normalize(&transaction.merchant), shown_processed);
    let alias = alias(&evidence);
    let brand = same_brand(&shown, &evidence)
        || alias.is_some_and(|alias| same_brand(&shown, &words(alias)));
    let (shown_numbers, evidence_numbers) = (numbers(&shown), numbers(&evidence));
    let numbers_differ = !shown_numbers.is_empty()
        && !evidence_numbers.is_empty()
        && !shown_numbers
            .iter()
            .any(|number| evidence_numbers.contains(number));
    // Plaid placed it in another city than the descriptor or the issuer names: Plaid's merchant
    // is somewhere else, so likely someone else.
    let reported = transaction
        .location
        .as_ref()
        .and_then(|location| location.city.as_deref())
        .map(places::normalize);
    let named = places::parse_purchase_descriptor(descriptor)
        .and_then(|place| place.city)
        .map(|city| places::normalize(&city.name))
        .or_else(|| {
            let issuer = transaction.issuer.as_ref()?;
            issuer.address.as_ref()?;
            issuer.city.as_deref().map(places::normalize)
        });
    let elsewhere = reported
        .zip(named)
        .is_some_and(|(reported, named)| reported != named);
    // Unrecognized, Plaid's name is the descriptor cleaned up; the issuer's fuller name is better.
    let fuller = !sure && issuer_name(transaction).is_some() && shown.concat() != evidence.concat();
    let contradicted = numbers_differ
        || (!brand && (elsewhere || !sure))
        || (!sure && !same_name(&shown, &evidence))
        || shown_processed
        || fuller;
    let name = alias.map_or_else(|| evidence.join(" "), str::to_owned);
    (contradicted && words(&name) != words(&transaction.merchant)).then_some(name)
}

#[cfg(test)]
mod tests;
