//! What the user's documents mean beside their accounts: each pay statement's deposit found in
//! a bank account; a W-2 predicted from the year's last pay statement and checked when it
//! comes; how long each document is worth keeping; and the year's income as the documents
//! state it.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;

use super::calendar;
use crate::documents::{Document, Kind};
use crate::finance_contract::{Account, Transaction};
use crate::numeric::{from_minor, minor_units, round2};
use crate::transaction_policy::Kind as TransactionKind;

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Documents {
    /// This tax year and last, newest first.
    pub years: Vec<TaxYear>,
    /// Each pay statement's deposits and the transactions they arrived as.
    pub pay: Vec<PayLink>,
    /// How long each document is worth keeping.
    pub keep: Vec<Keep>,
    /// Documents with a figure still to confirm.
    pub to_review: usize,
    /// Documents a newer reader would read better.
    pub outdated: Vec<String>,
    /// Transactions a document confirms: a pay statement's deposit, or a statement row.
    pub evidence: Vec<Evidence>,
}

/// A transaction a document states: the same amount, in the account the document names,
/// within days of the date it gives.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub transaction_id: String,
    pub document_id: String,
    /// `pay` for a pay statement's deposit, `statement` for a statement's row.
    pub kind: &'static str,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaxYear {
    pub year: i32,
    pub predictions: Vec<W2Prediction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub income: Option<Income>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct W2Prediction {
    pub employer: String,
    /// The pay statement the prediction comes from: the year's latest.
    pub from_document: String,
    pub from_date: NaiveDate,
    pub boxes: Vec<PredictedBox>,
    /// The W-2 it was checked against, once the user has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w2: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PredictedBox {
    pub key: String,
    pub label: String,
    pub predicted: f64,
    /// What the W-2 says, when it is in and states this box.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PayLink {
    pub document_id: String,
    pub date: NaiveDate,
    pub amount: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
    /// The transaction the deposit arrived as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    /// The account the statement names, when Brief tracks it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Keep {
    pub document_id: String,
    /// Safe to discard after this day; absent while it stays needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<NaiveDate>,
    pub reason: String,
}

/// The year's income as its documents state it, and what it suggests for the tax profile.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Income {
    /// W-2 box 1, or the pay statements' prediction of it.
    pub wages: f64,
    /// `w2` once every employer's W-2 is in; `payStatements` otherwise.
    pub wages_from: &'static str,
    /// Contractor pay: 1099-NEC box 1, else what arrived from contractors' payers.
    pub self_employment: f64,
    pub self_employment_tax: f64,
    /// Wages and self-employment less half that tax and the standard deduction; absent
    /// without a filing status or the year's tables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxable_income: Option<f64>,
}

/// Whether two names are the same payer: a shared word of four letters or more.
fn same_payer(left: &str, right: &str) -> bool {
    let words = |text: &str| {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.len() >= 4)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    };
    const COMMON: [&str; 9] = [
        "payroll", "deposit", "direct", "services", "service", "group", "company", "corp", "funds",
    ];
    let left = words(left);
    words(right)
        .iter()
        .any(|word| !COMMON.contains(&word.as_str()) && left.contains(word))
}

/// A payer's name from a deposit's descriptor, without the words banks add.
fn payer(transaction: &Transaction) -> Option<String> {
    let text = transaction.merchant.to_lowercase();
    const NOT_PAYERS: [&str; 12] = [
        "transfer", "trnsfr", "zelle", "venmo", "refund", "irs", "treas", "tax", "interest",
        "dividend", "chase", "cash app",
    ];
    if NOT_PAYERS.iter().any(|word| text.contains(word)) {
        return None;
    }
    let name = transaction
        .merchant
        .split_whitespace()
        .filter(|word| {
            let lower = word.to_lowercase();
            ![
                "payroll", "direct", "dep", "deposit", "funds", "recvd", "ppd", "id:",
            ]
            .contains(&lower.as_str())
                && word.chars().any(char::is_alphabetic)
        })
        .collect::<Vec<_>>()
        .join(" ");
    (name.chars().filter(|c| c.is_alphabetic()).count() >= 4).then_some(name)
}

fn payroll(transaction: &Transaction) -> bool {
    let text = transaction.descriptors().to_lowercase();
    text.contains("payroll") || text.contains("salary")
}

fn day(text: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text?, "%Y-%m-%d").ok()
}

pub struct Inputs<'a> {
    pub documents: &'a [Document],
    pub transactions: &'a [Transaction],
    pub accounts: &'a [Account],
    pub filing_status: Option<&'a str>,
}

pub fn documents(inputs: Inputs<'_>, today: NaiveDate) -> Documents {
    let Inputs {
        documents,
        transactions,
        accounts,
        filing_status,
    } = inputs;
    let pay = pay_links(documents, transactions, accounts);
    let years = [today.year(), today.year() - 1]
        .into_iter()
        .map(|year| tax_year(year, documents, transactions, filing_status, today))
        .collect();
    let mut evidence = pay
        .iter()
        .filter_map(|link| {
            Some(Evidence {
                transaction_id: link.transaction_id.clone()?,
                document_id: link.document_id.clone(),
                kind: "pay",
            })
        })
        .collect::<Vec<_>>();
    evidence.extend(statement_evidence(documents, transactions));
    Documents {
        years,
        pay,
        evidence,
        keep: documents.iter().map(keep).collect(),
        to_review: documents
            .iter()
            .filter(|document| !document.reviewed && !document.fields.is_empty())
            .count(),
        // What an older reader read. A statement already in history takes a new reading's
        // rows only when their days and amounts are unchanged.
        outdated: documents
            .iter()
            .filter(|document| {
                document.reader < crate::documents::READER_VERSION
                    && matches!(
                        document.kind,
                        Kind::Payslip | Kind::TaxForm | Kind::Statement
                    )
            })
            .map(|document| document.id.clone())
            .collect(),
    }
}

/// Each statement's rows matched to the account's transactions: the same signed amount, posted
/// within three days of the row. A row and a transaction each match once, the nearest first.
fn statement_evidence(documents: &[Document], transactions: &[Transaction]) -> Vec<Evidence> {
    let mut evidence = Vec::new();
    let mut used = BTreeSet::new();
    for document in documents
        .iter()
        .filter(|document| document.kind == Kind::Statement)
    {
        let (Some(account_id), Some(csv)) = (&document.account_id, &document.statement_csv) else {
            continue;
        };
        let Ok(parsed) = crate::statements::parse(csv.as_bytes()) else {
            continue;
        };
        let mut candidates = Vec::new();
        for (row, statement_row) in parsed.rows.iter().enumerate() {
            let amount = minor_units(statement_row.amount.try_into().unwrap_or(0.0));
            for transaction in transactions.iter().filter(|transaction| {
                !transaction.pending
                    && transaction.account_id.as_deref() == Some(account_id.as_str())
                    && minor_units(transaction.amount) == amount
            }) {
                let Some(posted) = calendar::transaction_date(
                    transaction
                        .posted_on
                        .as_deref()
                        .unwrap_or(&transaction.date),
                ) else {
                    continue;
                };
                let offset = (posted - statement_row.posted_on).num_days().abs();
                if offset <= 3 {
                    candidates.push((offset, row, transaction.id.as_str()));
                }
            }
        }
        candidates.sort();
        let mut rows = BTreeSet::new();
        for (_, row, id) in candidates {
            if !rows.contains(&row) && !used.contains(id) {
                rows.insert(row);
                used.insert(id.to_owned());
                evidence.push(Evidence {
                    transaction_id: id.to_owned(),
                    document_id: document.id.clone(),
                    kind: "statement",
                });
            }
        }
    }
    evidence
}

/// Each pay statement's deposits matched to transactions: the same amount within three days
/// before the check date to five after, in the account the statement names when Brief tracks
/// it. A transaction answers one deposit only.
fn pay_links(
    documents: &[Document],
    transactions: &[Transaction],
    accounts: &[Account],
) -> Vec<PayLink> {
    let mut used = BTreeSet::new();
    let mut links = Vec::new();
    let mut payslips = documents
        .iter()
        .filter(|document| document.kind == Kind::Payslip)
        .filter_map(|document| Some((day(document.date.as_deref())?, document)))
        .collect::<Vec<_>>();
    payslips.sort_by_key(|(date, document)| (*date, document.id.clone()));
    for (date, document) in payslips {
        for deposit in &document.deposits {
            let account = deposit.mask.as_deref().and_then(|mask| {
                accounts.iter().find(|account| {
                    account
                        .mask
                        .as_deref()
                        .is_some_and(|known| known.ends_with(mask) || mask.ends_with(known))
                })
            });
            let found = transactions
                .iter()
                .filter(|transaction| !transaction.pending && !used.contains(&transaction.id))
                .filter(|transaction| {
                    minor_units(transaction.amount) == minor_units(deposit.amount)
                })
                .filter(|transaction| {
                    account.is_none_or(|account| {
                        transaction.account_id.as_deref() == Some(account.id.as_str())
                    })
                })
                .filter_map(|transaction| {
                    let arrived = calendar::transaction_date(&transaction.date)?;
                    let offset = (arrived - date).num_days();
                    (-3..=5)
                        .contains(&offset)
                        .then_some((offset.abs(), transaction))
                })
                .min_by_key(|(offset, transaction)| (*offset, transaction.id.clone()))
                .map(|(_, transaction)| transaction.id.clone());
            if let Some(id) = &found {
                used.insert(id.clone());
            }
            links.push(PayLink {
                document_id: document.id.clone(),
                date,
                amount: deposit.amount,
                mask: deposit.mask.clone(),
                transaction_id: found,
                account_id: account.map(|account| account.id.clone()),
            });
        }
    }
    links
}

/// The year's latest pay statement from each employer, and what it says the W-2 will show.
fn predictions(year: i32, documents: &[Document]) -> Vec<W2Prediction> {
    let mut latest = BTreeMap::<String, &Document>::new();
    for document in documents.iter().filter(|document| {
        document.kind == Kind::Payslip
            && document.tax_year == Some(year)
            && document.issuer.is_some()
    }) {
        let employer = document.issuer.clone().unwrap_or_default();
        let key = latest
            .keys()
            .find(|known| same_payer(known, &employer))
            .cloned()
            .unwrap_or(employer);
        match latest.get(&key) {
            Some(known) if known.date >= document.date => {}
            _ => {
                latest.insert(key, document);
            }
        }
    }
    let wage_base = super::tax::social_security_wage_base(year);
    latest
        .into_iter()
        .filter_map(|(employer, document)| {
            let ytd = |key: &str| document.ytd(key).unwrap_or_default();
            let gross = document.ytd("gross")?;
            // Section 125 benefits and HSA contributions leave every wage box; 401(k) deferrals
            // leave box 1 only.
            let pretax = ytd("hsa") + ytd("pretaxBenefits");
            let box1 = gross - pretax - ytd("pretax401k");
            let social = (gross - pretax).min(wage_base.unwrap_or(f64::MAX));
            let mut boxes = vec![
                ("box1", "Wages, tips, other compensation", box1),
                (
                    "box2",
                    "Federal income tax withheld",
                    ytd("federalWithholding"),
                ),
                ("box3", "Social Security wages", social),
                (
                    "box4",
                    "Social Security tax withheld",
                    ytd("socialSecurityTax"),
                ),
                ("box5", "Medicare wages and tips", gross - pretax),
                ("box6", "Medicare tax withheld", ytd("medicareTax")),
                ("box16", "State wages", box1),
                ("box17", "State income tax", ytd("stateWithholding")),
            ];
            for (key, label, value) in [
                (
                    "box12:D",
                    "Box 12, code D (401(k) pre-tax)",
                    ytd("pretax401k"),
                ),
                ("box12:AA", "Box 12, code AA (Roth 401(k))", ytd("roth401k")),
                ("box12:W", "Box 12, code W (HSA)", ytd("hsa")),
            ] {
                if value > 0.0 {
                    boxes.push((key, label, value));
                }
            }
            let w2 = documents.iter().find(|candidate| {
                candidate.kind == Kind::TaxForm
                    && candidate
                        .form
                        .as_deref()
                        .is_some_and(|form| form.starts_with("W-2"))
                    && candidate.tax_year == Some(year)
                    && candidate
                        .issuer
                        .as_deref()
                        .is_some_and(|issuer| same_payer(issuer, &employer))
            });
            Some(W2Prediction {
                employer,
                from_document: document.id.clone(),
                from_date: day(document.date.as_deref())?,
                boxes: boxes
                    .into_iter()
                    .map(|(key, label, predicted)| PredictedBox {
                        key: key.into(),
                        label: label.into(),
                        predicted: round2(predicted),
                        actual: w2.and_then(|w2| w2.amount(key)),
                    })
                    .collect(),
                w2: w2.map(|w2| w2.id.clone()),
            })
        })
        .collect()
}

fn tax_year(
    year: i32,
    documents: &[Document],
    transactions: &[Transaction],
    filing_status: Option<&str>,
    today: NaiveDate,
) -> TaxYear {
    let posted = calendar::posted_through(transactions, today);
    let in_year = posted
        .iter()
        .filter(|(date, _)| date.year() == year)
        .map(|(_, transaction)| *transaction)
        .collect::<Vec<_>>();

    // Each employer: one whose pay statements or W-2 Brief has, or whose payroll arrived.
    let mut employers = Vec::<(String, &'static str)>::new();
    for document in documents.iter().filter(|document| {
        document.tax_year == Some(year)
            && (document.kind == Kind::Payslip
                || document
                    .form
                    .as_deref()
                    .is_some_and(|form| form.starts_with("W-2")))
    }) {
        if let Some(issuer) = &document.issuer {
            if !employers.iter().any(|(known, _)| same_payer(known, issuer)) {
                employers.push((
                    issuer.clone(),
                    if document.kind == Kind::Payslip {
                        "Pay statements"
                    } else {
                        "Your W-2"
                    },
                ));
            }
        }
    }
    let known_employers = documents
        .iter()
        .filter(|document| {
            document.kind == Kind::Payslip
                || document
                    .form
                    .as_deref()
                    .is_some_and(|form| form.starts_with("W-2"))
        })
        .filter_map(|document| document.issuer.clone())
        .collect::<Vec<_>>();
    let mut contractors = BTreeMap::<String, i64>::new();
    for transaction in in_year.iter().filter(|transaction| {
        transaction.kind() == Some(TransactionKind::Income) && transaction.amount > 0.0
    }) {
        let Some(name) = payer(transaction) else {
            continue;
        };
        if payroll(transaction) {
            // Named as a known employer when one matches, so it isn't listed twice.
            let name = known_employers
                .iter()
                .find(|known| same_payer(known, &name))
                .cloned()
                .unwrap_or(name);
            if !employers.iter().any(|(known, _)| same_payer(known, &name)) {
                employers.push((name, "Payroll deposits"));
            }
        } else if !known_employers.iter().any(|known| same_payer(known, &name)) {
            let key = contractors
                .keys()
                .find(|known| same_payer(known, &name))
                .cloned()
                .unwrap_or(name);
            *contractors.entry(key).or_default() += minor_units(transaction.amount);
        }
    }
    let predictions = predictions(year, documents);
    let income = income(
        year,
        documents,
        &predictions,
        &employers,
        &contractors,
        filing_status,
    );
    TaxYear {
        year,
        predictions,
        income,
    }
}

/// Wages from W-2s, or predicted from pay statements while any is missing; contractor pay from
/// 1099-NECs, or what arrived from a payer with none; and self-employment tax on it.
fn income(
    year: i32,
    documents: &[Document],
    predictions: &[W2Prediction],
    employers: &[(String, &'static str)],
    contractors: &BTreeMap<String, i64>,
    filing_status: Option<&str>,
) -> Option<Income> {
    let w2s = documents
        .iter()
        .filter(|document| {
            document.tax_year == Some(year)
                && document
                    .form
                    .as_deref()
                    .is_some_and(|form| form.starts_with("W-2"))
        })
        .collect::<Vec<_>>();
    let all_in = !employers.is_empty()
        && employers.iter().all(|(employer, _)| {
            w2s.iter().any(|w2| {
                w2.issuer
                    .as_deref()
                    .is_some_and(|issuer| same_payer(issuer, employer))
            })
        });
    let (wages, social_wages, wages_from) = if all_in {
        (
            w2s.iter().filter_map(|w2| w2.amount("box1")).sum::<f64>(),
            w2s.iter().filter_map(|w2| w2.amount("box3")).sum::<f64>(),
            "w2",
        )
    } else {
        let predicted = |key: &str| {
            predictions
                .iter()
                .filter_map(|prediction| prediction.boxes.iter().find(|item| item.key == key))
                .map(|item| item.actual.unwrap_or(item.predicted))
                .sum::<f64>()
        };
        // An employer whose W-2 is in but who sent no pay statements counts from the W-2.
        let unpredicted = w2s
            .iter()
            .filter(|w2| {
                !predictions.iter().any(|prediction| {
                    w2.issuer
                        .as_deref()
                        .is_some_and(|issuer| same_payer(issuer, &prediction.employer))
                })
            })
            .collect::<Vec<_>>();
        (
            predicted("box1")
                + unpredicted
                    .iter()
                    .filter_map(|w2| w2.amount("box1"))
                    .sum::<f64>(),
            predicted("box3")
                + unpredicted
                    .iter()
                    .filter_map(|w2| w2.amount("box3"))
                    .sum::<f64>(),
            "payStatements",
        )
    };
    let nec = documents
        .iter()
        .filter(|document| {
            document.kind == Kind::TaxForm
                && document.tax_year == Some(year)
                && document
                    .form
                    .as_deref()
                    .is_some_and(|form| form.starts_with("1099-NEC"))
        })
        .collect::<Vec<_>>();
    let arrived = contractors
        .iter()
        .filter(|(name, _)| {
            !nec.iter().any(|form| {
                form.issuer
                    .as_deref()
                    .is_some_and(|issuer| same_payer(issuer, name))
            })
        })
        .map(|(_, cents)| from_minor(*cents))
        .filter(|paid| *paid >= 100.0)
        .sum::<f64>();
    let self_employment = nec
        .iter()
        .filter_map(|form| form.amount("box1"))
        .sum::<f64>()
        + arrived;
    if wages <= 0.0 && self_employment <= 0.0 {
        return None;
    }
    // Self-employment tax: 15.3% of 92.35% of net earnings, the Social Security part only up to
    // what the year's wages left of the wage base; none under $400.
    let net = self_employment * 0.9235;
    let self_employment_tax = if net < 400.0 {
        0.0
    } else {
        let room = super::tax::social_security_wage_base(year)
            .map_or(net, |base| (base - social_wages).max(0.0));
        round2(net.min(room) * 0.124 + net * 0.029)
    };
    let taxable_income = filing_status
        .and_then(|status| super::tax::standard_deduction(year, status))
        .map(|deduction| {
            round2((wages + self_employment - self_employment_tax / 2.0 - deduction).max(0.0))
        });
    Some(Income {
        wages: round2(wages),
        wages_from,
        self_employment: round2(self_employment),
        self_employment_tax,
        taxable_income,
    })
}

/// How long a document is worth keeping. Tax forms and statements: three years after the
/// return for their year is due. Pay statements: until the year's W-2 agrees with them. Forms
/// that prove what was paid for shares or put into a Roth: while the account or shares are held.
fn keep(document: &Document) -> Keep {
    let filed = |year: i32| NaiveDate::from_ymd_opt(year + 4, 4, 15);
    let (until, reason) = match (document.kind, document.form.as_deref()) {
        (Kind::TaxForm, Some("5498")) => (
            None,
            "Proves Roth contributions; keep while the IRA is open",
        ),
        (Kind::TaxForm, Some("3922")) => (
            None,
            "Proves what the shares cost; keep until three years after selling them",
        ),
        (Kind::Payslip, _) => (None, "Keep until the year's W-2 matches it"),
        (Kind::TaxForm | Kind::TaxReturn | Kind::Statement | Kind::Export, _) => (
            document.tax_year.and_then(filed),
            "Three years after that year's return is due",
        ),
        (Kind::Other, _) => (None, "Keep as long as it's useful"),
    };
    Keep {
        document_id: document.id.clone(),
        until,
        reason: reason.into(),
    }
}

/// What calls for the user: pay that didn't arrive where its statement says, and a W-2 that
/// disagrees with the year's pay statements.
pub fn needs(
    documents: &Documents,
    all: &[Document],
    today: NaiveDate,
) -> Vec<super::attention::AttentionItem> {
    use super::attention::{AttentionItem, Item, Severity};
    let mut needs = Vec::new();
    for link in documents.pay.iter().filter(|link| {
        link.transaction_id.is_none()
            && link.account_id.is_some()
            && today > link.date + Duration::days(5)
    }) {
        let employer = all
            .iter()
            .find(|document| document.id == link.document_id)
            .and_then(|document| document.issuer.clone())
            .unwrap_or_else(|| "Your employer".into());
        needs.push(AttentionItem {
            id: format!(
                "pay-missing:{}:{}",
                link.document_id,
                minor_units(link.amount)
            ),
            severity: Severity::Attention,
            item: Item::PayMissing {
                document_id: link.document_id.clone(),
                employer,
                date: link.date,
                amount: link.amount,
                account_id: link.account_id.clone(),
            },
        });
    }
    for year in &documents.years {
        for prediction in &year.predictions {
            let differs = prediction
                .boxes
                .iter()
                .filter(|item| {
                    item.actual
                        .is_some_and(|actual| (actual - item.predicted).abs() > 1.0)
                })
                .count();
            if let (Some(w2), true) = (&prediction.w2, differs > 0) {
                needs.push(AttentionItem {
                    id: format!("w2-differs:{w2}"),
                    severity: Severity::Attention,
                    item: Item::W2Differs {
                        document_id: w2.clone(),
                        employer: prediction.employer.clone(),
                        tax_year: year.year,
                        boxes: differs,
                    },
                });
            }
        }
    }
    needs
}

#[cfg(test)]
mod tests;
