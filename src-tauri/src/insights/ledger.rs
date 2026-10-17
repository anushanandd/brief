//! Read-only ledger queries over the annotation-aware snapshot. They are the only figures
//! Apple Intelligence may cite: every amount is computed here, in whole cents.
use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};

use super::calendar;
use super::{Insights, View};
use crate::finance_contract::Transaction;
use crate::numeric::{from_minor, minor_units};
use crate::transaction_policy::Kind;

const MAX_ROWS: usize = 25;

/// Every tool, with words a question that needs it is likely to use. The on-device model
/// reads about 4,096 tokens per question, and every tool's definition takes some of them, so
/// a question is offered only the tools its words point to.
pub const TOOLS: [(&str, &[&str]); 15] = [
    (
        "listAccounts",
        &[
            "account",
            "accounts",
            "balance",
            "checking",
            "savings",
            "cashplus",
            "brokerage",
            "worth",
            "bank",
            "issue",
            "issues",
            "institution",
        ],
    ),
    (
        "spendingByCategory",
        &[
            "category",
            "categories",
            "on",
            "food",
            "dining",
            "groceries",
            "rent",
            "transportation",
            "drink",
            "shopping",
        ],
    ),
    (
        "cashFlow",
        &[
            "earn", "earned", "income", "net", "spend", "spent", "flow", "made", "salary", "save",
            "saved",
        ],
    ),
    (
        "searchTransactions",
        &[
            "when",
            "transaction",
            "charge",
            "charged",
            "purchase",
            "bought",
            "pay",
            "paid",
            "bonus",
            "fee",
            "flight",
            "furniture",
            "post",
            "posted",
        ],
    ),
    (
        "merchantHistory",
        &["paid", "merchant", "history", "every", "monthly", "much"],
    ),
    (
        "upcomingActivity",
        &[
            "next", "upcoming", "due", "expected", "paycheck", "bill", "bills", "coming",
        ],
    ),
    (
        "monthPlan",
        &[
            "month", "cash", "runway", "typical", "budget", "far", "cover", "covers",
        ],
    ),
    (
        "topHoldings",
        &[
            "holding",
            "holdings",
            "stock",
            "stocks",
            "portfolio",
            "invested",
            "position",
        ],
    ),
    (
        "creditCards",
        &[
            "card",
            "cards",
            "owe",
            "utilization",
            "limit",
            "statement",
            "credit",
        ],
    ),
    (
        "taxSummary",
        &["tax", "taxes", "gains", "wash", "taxable", "interest"],
    ),
    (
        "trips",
        &["trip", "trips", "travel", "traveled", "vacation", "away"],
    ),
    (
        "subscriptions",
        &["subscription", "subscriptions", "streaming"],
    ),
    ("dividendIncome", &["dividend", "dividends"]),
    (
        "explanations",
        &[
            "why",
            "waive",
            "waived",
            "waiver",
            "bonus",
            "rule",
            "rules",
            "source",
            "offset",
            "conditions",
            "confirm",
            "requirement",
            "wrong",
            "deposits",
        ],
    ),
    (
        "chapters",
        &[
            "live",
            "lived",
            "work",
            "job",
            "move",
            "moved",
            "joined",
            "join",
            "start",
            "started",
            "leave",
            "left",
            "state",
            "states",
            "residency",
            "chapter",
            "since",
            "change",
            "opened",
            "open",
        ],
    ),
];

fn words_of(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2)
        .map(str::to_owned)
        .collect()
}

/// The tools offered for a question: those its words point to most, at most four. Names the
/// saved data holds point too: a trip's city to trips, a chapter's title to chapters, an
/// account's name to accounts. A question that points to none gets the general ones.
pub fn tools_for(question: &str, view: &View) -> Vec<&'static str> {
    let words = words_of(question);
    let named = |names: Vec<String>| {
        let known = names
            .iter()
            .flat_map(|name| words_of(name))
            .collect::<Vec<_>>();
        words.iter().filter(|word| known.contains(word)).count()
    };
    let insights = view.insights.as_ref();
    let data = [
        (
            "trips",
            named(insights.map_or_else(Vec::new, |insights| {
                insights
                    .trips
                    .trips
                    .iter()
                    .map(|trip| trip.destination.clone())
                    .collect()
            })),
        ),
        (
            "chapters",
            named(insights.map_or_else(Vec::new, |insights| {
                insights
                    .evidence
                    .chapters
                    .iter()
                    .map(|chapter| chapter.title.clone())
                    .collect()
            })),
        ),
        (
            "listAccounts",
            named(
                view.snapshot
                    .accounts
                    .iter()
                    .map(|account| account.name.clone())
                    .collect(),
            ),
        ),
    ];
    let mut scored = TOOLS
        .iter()
        .enumerate()
        .map(|(order, (name, hints))| {
            let hinted = words
                .iter()
                .filter(|word| hints.contains(&word.as_str()))
                .count();
            let from_data = data
                .iter()
                .find(|(tool, _)| tool == name)
                .map_or(0, |(_, count)| *count * 2);
            (hinted + from_data, order, *name)
        })
        .filter(|(score, _, _)| *score > 0)
        .collect::<Vec<_>>();
    scored.sort_by_key(|(score, order, _)| (std::cmp::Reverse(*score), *order));
    let mut tools = scored
        .into_iter()
        .take(4)
        .map(|(_, _, name)| name)
        .collect::<Vec<_>>();
    if tools.is_empty() {
        tools = vec!["listAccounts", "cashFlow", "searchTransactions", "chapters"];
    }
    tools
}

fn to_json(value: &impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

pub struct Ledger<'a> {
    view: &'a View,
    pub today: NaiveDate,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Range {
    start: Option<String>,
    end: Option<String>,
}

fn parse_date(value: Option<&str>) -> Option<NaiveDate> {
    value.and_then(|value| NaiveDate::parse_from_str(value.get(..10)?, "%Y-%m-%d").ok())
}

impl<'a> Ledger<'a> {
    pub fn new(view: &'a View, today: NaiveDate) -> Self {
        Self { view, today }
    }

    fn reference(&self) -> &str {
        &self.view.snapshot.updated_at
    }

    fn date(&self, transaction: &Transaction) -> Option<NaiveDate> {
        calendar::transaction_date(transaction.posted_date())
    }

    fn account_name(&self, id: Option<&str>) -> String {
        id.and_then(|id| self.view.account_name(id))
            .unwrap_or("Unknown account")
            .to_owned()
    }

    /// The derived insights, which a snapshot whose insights failed doesn't have.
    fn insights(&self) -> Result<&'a Insights, String> {
        self.view
            .insights
            .as_ref()
            .ok_or_else(|| "Brief's insights are unavailable for this snapshot".into())
    }

    /// A date range, defaulting to the current month to date and capped at five years.
    fn range(&self, arguments: &Value) -> Result<(NaiveDate, NaiveDate), String> {
        let range: Range = serde_json::from_value(arguments.clone()).unwrap_or_default();
        let end = parse_date(range.end.as_deref())
            .unwrap_or(self.today)
            .min(self.today);
        let start = parse_date(range.start.as_deref())
            .unwrap_or_else(|| calendar::add_months_first(self.today, 0));
        if start > end {
            return Err("The start date is after the end date".into());
        }
        Ok((start.max(end - Duration::days(366 * 5)), end))
    }

    fn posted_in(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> impl Iterator<Item = (NaiveDate, &Transaction)> {
        self.view
            .snapshot
            .transactions
            .iter()
            .filter_map(move |transaction| {
                let date = self.date(transaction)?;
                (!transaction.pending && date >= start && date <= end)
                    .then_some((date, transaction))
            })
    }

    /// Runs a tool by name. Unknown tools and invalid arguments return an error message the
    /// model can relay, never a guessed value.
    pub fn call(&self, name: &str, arguments: &Value) -> Value {
        let result = match name {
            "listAccounts" => Ok(self.list_accounts()),
            "spendingByCategory" => self.spending_by_category(arguments),
            "cashFlow" => self.cash_flow(arguments),
            "searchTransactions" => self.search_transactions(arguments),
            "merchantHistory" => self.merchant_history(arguments),
            "upcomingActivity" => self.upcoming(arguments),
            "monthPlan" => self.insights().and_then(|insights| to_json(&insights.plan)),
            "topHoldings" => Ok(self.top_holdings(arguments)),
            "creditCards" => self.credit_cards(),
            "taxSummary" => self.tax_summary(),
            "trips" => self.trips(arguments),
            "subscriptions" => self.subscriptions(),
            "dividendIncome" => self.dividend_income(),
            "explanations" => self.explanations(),
            "chapters" if arguments["name"].is_string() => self.chapter_span(arguments),
            "chapters" => self.chapters(),
            _ => Err(format!("Unknown tool {name}")),
        };
        result.unwrap_or_else(|error| json!({ "error": error }))
    }

    fn list_accounts(&self) -> Value {
        json!({
            "savedAt": self.reference(),
            "accounts": self.view.snapshot.accounts
                .iter()
                .filter(|account| account.id != "all")
                .map(|account| json!({
                    "name": self.account_name(Some(&account.id)),
                    "institution": account.institution,
                    "type": account.r#type,
                    "balance": account.value,
                    "currency": account.currency,
                }))
                .collect::<Vec<_>>(),
        })
    }

    fn spending_by_category(&self, arguments: &Value) -> Result<Value, String> {
        let (start, end) = self.range(arguments)?;
        let mut categories = BTreeMap::<String, (i64, usize)>::new();
        for (_, transaction) in self.posted_in(start, end) {
            match transaction.kind() {
                Some(Kind::Expense) if transaction.amount < 0.0 => {}
                Some(Kind::Reimbursement) if transaction.amount > 0.0 => {}
                _ => continue,
            }
            for (category, amount) in transaction.parts() {
                let entry = categories.entry(category.to_owned()).or_default();
                entry.0 -= minor_units(amount);
                entry.1 += 1;
            }
        }
        let mut rows = categories.into_iter().collect::<Vec<_>>();
        rows.sort_by_key(|(_, (amount, _))| std::cmp::Reverse(*amount));
        let total = rows.iter().map(|(_, (amount, _))| amount).sum::<i64>();
        Ok(json!({
            "start": calendar::iso(start),
            "end": calendar::iso(end),
            "totalSpending": from_minor(total.max(0)),
            "categories": rows.into_iter().map(|(category, (amount, count))| json!({
                "category": category,
                "spending": from_minor(amount),
                "transactions": count,
            })).collect::<Vec<_>>(),
        }))
    }

    fn cash_flow(&self, arguments: &Value) -> Result<Value, String> {
        let (start, end) = self.range(arguments)?;
        let (mut income, mut spending, mut reimbursements, mut fees, mut taxes) = (0, 0, 0, 0, 0);
        for (_, transaction) in self.posted_in(start, end) {
            let amount = minor_units(transaction.amount);
            match transaction.kind() {
                Some(Kind::Income | Kind::Dividend | Kind::Interest) if amount > 0 => {
                    income += amount
                }
                Some(Kind::Expense) => spending -= amount,
                Some(Kind::Reimbursement) if amount > 0 => reimbursements += amount,
                Some(Kind::Fee) => fees -= amount,
                Some(Kind::Tax) => taxes -= amount,
                _ => {}
            }
        }
        let personal = (spending - reimbursements).max(0);
        Ok(json!({
            "start": calendar::iso(start),
            "end": calendar::iso(end),
            "income": from_minor(income),
            "spendingAfterReimbursements": from_minor(personal),
            "fees": from_minor(fees),
            "taxes": from_minor(taxes),
            "netCashFlow": from_minor(income - personal - fees - taxes),
        }))
    }

    fn search_transactions(&self, arguments: &Value) -> Result<Value, String> {
        let text = arguments["text"]
            .as_str()
            .map(str::to_lowercase)
            .filter(|text| !text.is_empty());
        // Without a start, the search covers the year to its end, as the tool says. Today alone with
        // words to find is the model filling both dates with today, which finds nothing; it
        // widens to the last year too, and the result says so.
        let year_back = self.today - Duration::days(366);
        let (mut start, end) = match parse_date(arguments["start"].as_str()) {
            Some(_) => self.range(arguments)?,
            None => {
                let end = parse_date(arguments["end"].as_str())
                    .unwrap_or(self.today)
                    .min(self.today);
                (end - Duration::days(366), end)
            }
        };
        let widened = text.is_some() && start == self.today && end == self.today;
        if widened {
            start = year_back;
        }
        // A category no saved transaction has (the model guesses names like "Bonus") would
        // find nothing; it is left out, and the result says so.
        let asked = arguments["category"].as_str().map(str::to_lowercase);
        let category = asked.clone().filter(|category| {
            self.view
                .snapshot
                .transactions
                .iter()
                .any(|transaction| transaction.category.to_lowercase() == *category)
        });
        let limit = arguments["limit"]
            .as_u64()
            .map_or(10, |limit| limit as usize)
            .clamp(1, MAX_ROWS);
        let mut matches = self
            .view
            .snapshot
            .transactions
            .iter()
            .filter_map(|transaction| {
                let date = self.date(transaction)?;
                let haystack = format!(
                    "{} {}",
                    transaction.merchant,
                    transaction.description.as_deref().unwrap_or("")
                )
                .to_lowercase();
                (date >= start
                    && date <= end
                    && text
                        .as_ref()
                        .is_none_or(|text| haystack.contains(text.as_str()))
                    && category
                        .as_ref()
                        .is_none_or(|category| transaction.category.to_lowercase() == *category))
                .then_some((date, transaction))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|(date, _)| std::cmp::Reverse(*date));
        let total = matches.len();
        let mut result = json!({
            "matches": total,
            "shown": total.min(limit),
            "transactions": matches.into_iter().take(limit).map(|(date, transaction)| json!({
                "date": calendar::iso(date),
                "merchant": transaction.merchant,
                "category": transaction.category,
                "amount": transaction.amount,
                "account": self.account_name(transaction.account_id.as_deref()),
                "pending": transaction.pending,
            })).collect::<Vec<_>>(),
        });
        if let Some(asked) = asked.filter(|_| category.is_none()) {
            result["ignoredCategory"] = json!(asked);
        }
        if widened {
            result["searchedFrom"] = json!(calendar::iso(start));
        }
        Ok(result)
    }

    fn merchant_history(&self, arguments: &Value) -> Result<Value, String> {
        let merchant = arguments["merchant"]
            .as_str()
            .map(str::to_lowercase)
            .filter(|merchant| merchant.trim().len() >= 2)
            .ok_or("Name a merchant")?;
        let start = calendar::add_months_first(self.today, -11);
        let mut months = BTreeMap::<NaiveDate, (i64, usize)>::new();
        for (date, transaction) in self.posted_in(start, self.today) {
            if !transaction
                .merchant
                .to_lowercase()
                .contains(merchant.trim())
            {
                continue;
            }
            let entry = months
                .entry(calendar::add_months_first(date, 0))
                .or_default();
            entry.0 += minor_units(transaction.amount);
            entry.1 += 1;
        }
        let count = months.values().map(|(_, count)| count).sum::<usize>();
        let total = months.values().map(|(amount, _)| amount).sum::<i64>();
        Ok(json!({
            "merchant": merchant,
            "from": calendar::iso(start),
            "transactions": count,
            "netAmount": from_minor(total),
            "months": months.into_iter().map(|(month, (amount, count))| json!({
                "month": format!("{}-{:02}", month.year(), month.month()),
                "netAmount": from_minor(amount),
                "transactions": count,
            })).collect::<Vec<_>>(),
        }))
    }

    fn upcoming(&self, arguments: &Value) -> Result<Value, String> {
        let days = arguments["days"].as_i64().unwrap_or(30).clamp(1, 60);
        let horizon = self.today + Duration::days(days);
        let events = self
            .insights()?
            .expected
            .iter()
            .filter(|event| event.date <= horizon)
            .map(|event| {
                json!({
                    "title": event.title,
                    "date": event.date,
                    "estimatedAmount": event.amount,
                    "frequency": event.frequency,
                    "state": event.state,
                    "account": self.account_name(event.account_id.as_deref()),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({ "throughDate": calendar::iso(horizon), "estimates": events }))
    }

    fn credit_cards(&self) -> Result<Value, String> {
        let credit = &self.insights()?.credit;
        Ok(json!({
            "utilizationPercent": credit.utilization,
            "balance": credit.balance,
            "limit": credit.limit,
            "cardsWithoutLimit": credit.cards_without_limit,
            "interestCharged": credit.interest_charged,
            "cards": credit.cards.iter().take(MAX_ROWS).map(|card| json!({
                "card": self.account_name(Some(&card.account_id)),
                "balance": card.balance,
                "limit": card.limit,
                "utilizationPercent": card.utilization,
                "statementCloses": card.closes_on,
                "interestCharged": card.interest_charged,
            })).collect::<Vec<_>>(),
        }))
    }

    fn tax_summary(&self) -> Result<Value, String> {
        let taxes = &self.insights()?.taxes;
        let year = taxes
            .years
            .first()
            .ok_or("No taxable investment activity is saved")?;
        let estimate = year.estimate.as_ref();
        Ok(json!({
            "year": year.year,
            "interest": year.interest,
            "dividends": year.dividends.total,
            "qualifiedDividends": year.dividends.qualified,
            "shortTermGains": year.realized.short_term,
            "longTermGains": year.realized.long_term,
            "washSaleDisallowed": year.realized.wash_disallowed,
            "estimatedTaxLow": estimate.map(|estimate| estimate.low),
            "estimatedTaxHigh": estimate.map(|estimate| estimate.high),
            "hasTaxProfile": taxes.has_profile,
            "partialYear": year.partial,
            "openWashSaleWindows": taxes.wash_windows.iter().take(MAX_ROWS).map(|window| json!({
                "ticker": window.ticker,
                "avoidBuyingThrough": window.until,
                "loss": window.loss,
            })).collect::<Vec<_>>(),
        }))
    }

    fn trips(&self, arguments: &Value) -> Result<Value, String> {
        let limit = arguments["limit"]
            .as_u64()
            .map_or(10, |limit| limit as usize)
            .clamp(1, MAX_ROWS);
        Ok(json!({
            "trips": self.insights()?.trips.trips.iter().take(limit).map(|trip| json!({
                "destination": trip.destination,
                "start": trip.start,
                "end": trip.end,
                "total": trip.total,
                "perDay": trip.per_day,
                "transactions": trip.transaction_ids.len(),
            })).collect::<Vec<_>>(),
        }))
    }

    fn subscriptions(&self) -> Result<Value, String> {
        let subscriptions = &self.insights()?.subscriptions;
        Ok(json!({
            "monthlyTotal": subscriptions.monthly,
            "subscriptions": subscriptions.items.iter().take(MAX_ROWS).map(|item| json!({
                "name": item.title,
                "amount": item.amount,
                "frequency": item.frequency,
                "nextDate": item.next_date,
                "previousAmount": item.price_change.as_ref().map(|change| change.previous),
                "priceChangedOn": item.price_change.as_ref().map(|change| change.changed_on),
            })).collect::<Vec<_>>(),
        }))
    }

    fn dividend_income(&self) -> Result<Value, String> {
        let dividends = &self.insights()?.dividends;
        Ok(json!({
            "annualIncome": dividends.annual_income,
            "upcoming": dividends.upcoming.iter().take(MAX_ROWS).map(|next| json!({
                "ticker": next.ticker,
                "exDate": next.dividend.ex_date,
                "payableDate": next.dividend.payable_date,
                "amount": next.dividend.amount,
            })).collect::<Vec<_>>(),
        }))
    }

    /// What published rules explain, each with its tier, its conditions, and its source, and
    /// the notes rules raise. A Suggested conclusion is a possibility, never a fact.
    fn explanations(&self) -> Result<Value, String> {
        let evidence = &self.insights()?.evidence;
        Ok(json!({
            "explanations": evidence.explanations.iter().take(MAX_ROWS).map(|explanation| json!({
                "title": explanation.title,
                "says": explanation.says,
                "tier": explanation.tier,
                "from": explanation.from,
                "to": explanation.to,
                "rule": explanation.proof.name,
                "source": explanation.proof.source.url,
                "conditions": explanation.proof.steps.iter().map(|step| json!({
                    "says": step.says,
                    "met": step.met,
                    "detail": step.detail,
                })).collect::<Vec<_>>(),
                "unchecked": explanation.proof.unchecked,
            })).collect::<Vec<_>>(),
            "notes": evidence.alerts.iter().map(|alert| &alert.says).collect::<Vec<_>>(),
        }))
    }

    /// Life chapters, where you are now, and where you lived each tax year.
    fn chapters(&self) -> Result<Value, String> {
        let evidence = &self.insights()?.evidence;
        let effect = |chapter: &crate::evidence::chapters::Chapter, measure: &str| {
            chapter
                .effects
                .iter()
                .find(|effect| effect.measure == measure)
                .map(|effect| json!({ "before": effect.before, "after": effect.after }))
        };
        Ok(json!({
            "now": evidence.now,
            "chapters": evidence.chapters.iter().rev().take(MAX_ROWS).map(|chapter| json!({
                "title": chapter.title,
                "kind": chapter.kind,
                "start": chapter.start,
                "end": chapter.end,
                "tier": chapter.tier,
                "state": chapter.state,
                "programs": chapter.programs,
                "checklist": chapter.checklist,
                "monthlyIncome": effect(chapter, "income"),
                "monthlySpending": effect(chapter, "spending"),
            })).collect::<Vec<_>>(),
            "residency": evidence.residency.iter().map(|span| json!({
                "year": span.year,
                "state": span.state,
                "from": span.from,
                "to": span.to,
                "days": span.days,
            })).collect::<Vec<_>>(),
        }))
    }

    /// `chapters` given a name: the dates a phrase like "since I moved" or "since joining
    /// Morgan Stanley" means, kept small for the on-device model's context. A
    /// move or home is the latest home chapter, a job the latest work chapter, and anything
    /// else the chapter whose title or state names it.
    fn chapter_span(&self, arguments: &Value) -> Result<Value, String> {
        use crate::evidence::chapters::{state_name, ChapterKind};
        let evidence = &self.insights()?.evidence;
        let name = arguments["name"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty() && name.len() <= 100)
            .ok_or("Name the chapter, such as a move, a job, or an employer")?
            .to_lowercase();
        let words = name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.len() > 1)
            .collect::<Vec<_>>();
        let has = |options: &[&str]| words.iter().any(|word| options.contains(word));
        let latest = |kind: ChapterKind| {
            evidence
                .chapters
                .iter()
                .filter(|chapter| chapter.kind == kind)
                .max_by_key(|chapter| chapter.start)
        };
        // The chapter naming the most of the phrase's words, the latest when tied.
        let named = evidence
            .chapters
            .iter()
            .map(|chapter| {
                let title = chapter.title.to_lowercase();
                let state = chapter
                    .state
                    .as_deref()
                    .and_then(state_name)
                    .unwrap_or_default()
                    .to_lowercase();
                let score = words
                    .iter()
                    .filter(|word| {
                        ![
                            "the", "of", "at", "in", "since", "my", "job", "work", "joined",
                            "joining",
                        ]
                        .contains(*word)
                            && (title
                                .split_whitespace()
                                .any(|part| part.starts_with(**word))
                                || state.split_whitespace().any(|part| part == **word))
                    })
                    .count();
                (score, chapter.start, chapter)
            })
            .filter(|(score, _, _)| *score > 0)
            .max_by_key(|(score, start, _)| (*score, *start))
            .map(|(_, _, chapter)| chapter);
        let chapter = named
            .or_else(|| {
                has(&["move", "moved", "moving", "home", "lived", "live"])
                    .then(|| latest(ChapterKind::Home))
                    .flatten()
            })
            .or_else(|| {
                has(&["job", "work", "joined", "joining", "started", "employer"])
                    .then(|| latest(ChapterKind::Work))
                    .flatten()
            })
            .ok_or("No chapter matches that name")?;
        Ok(json!({
            "title": chapter.title,
            "kind": chapter.kind,
            "start": chapter.start,
            "end": chapter.end,
            "tier": chapter.tier,
        }))
    }

    fn top_holdings(&self, arguments: &Value) -> Value {
        let limit = arguments["limit"]
            .as_u64()
            .map_or(10, |limit| limit as usize)
            .clamp(1, MAX_ROWS);
        let mut holdings = self.view.snapshot.holdings.iter().collect::<Vec<_>>();
        holdings.sort_by(|left, right| {
            right
                .value
                .unwrap_or(f64::MIN)
                .total_cmp(&left.value.unwrap_or(f64::MIN))
        });
        json!({
            "holdings": holdings.into_iter().take(limit).map(|holding| json!({
                "ticker": holding.ticker,
                "name": holding.name,
                "value": holding.value,
                "totalChangePercent": holding.total_change_pct,
                "account": self.account_name(Some(&holding.account_id)),
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::calendar::NamedZone;

    /// The synthetic seed as read, with a name given to Checking, and its insights on request.
    fn view(insights: bool) -> View {
        let mut snapshot: crate::finance_contract::Snapshot =
            serde_json::from_str(include_str!("../../../src/data/seed.json")).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-31T21:42:00Z")
            .unwrap()
            .to_utc();
        let insights = insights.then(|| {
            crate::insights::derive(
                &mut snapshot,
                &crate::settings::FinanceSettings::default(),
                now,
                &NamedZone::Utc,
            )
            .unwrap()
        });
        View {
            snapshot,
            insights,
            account_names: BTreeMap::from([("chase".into(), "Everyday".into())]),
            spending_account_id: None,
        }
    }

    #[test]
    fn queries_sum_classified_activity_in_cents_and_reject_bad_input() {
        let view = view(false);
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        let ledger = Ledger::new(&view, today);
        // Without insights, the tools that quote them say so.
        assert!(ledger.call("creditCards", &json!({}))["error"].is_string());
        let spending = ledger.call(
            "spendingByCategory",
            &json!({ "start": "2026-07-01", "end": "2026-07-31" }),
        );
        let entertainment = spending["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["category"] == "Entertainment")
            .unwrap()
            .clone();
        // July: Spotify 11.99, Netflix 15.49, Disney Plus 13.99.
        assert_eq!(entertainment["spending"], 41.47);
        assert_eq!(entertainment["transactions"], 3);
        let flow = ledger.call(
            "cashFlow",
            &json!({ "start": "2026-07-01", "end": "2026-07-31" }),
        );
        assert_eq!(flow["income"], 4_821.75 * 3.0);
        let search = ledger.call(
            "searchTransactions",
            &json!({ "text": "netflix", "limit": 2 }),
        );
        assert_eq!(search["matches"], 3);
        assert_eq!(search["shown"], 2);
        // A category no transaction has is left out rather than finding nothing.
        let guessed = ledger.call(
            "searchTransactions",
            &json!({ "text": "netflix", "category": "Streaming Bonus" }),
        );
        assert_eq!(guessed["matches"], 3);
        assert_eq!(guessed["ignoredCategory"], "streaming bonus");
        assert!(search.get("ignoredCategory").is_none());
        // Without a start, the year before the end; today alone, with words, the last year.
        let ended = ledger.call(
            "searchTransactions",
            &json!({ "text": "netflix", "end": "2026-08-31" }),
        );
        assert_eq!(ended["matches"], 3);
        let today = ledger.call(
            "searchTransactions",
            &json!({ "text": "netflix", "start": "2026-08-31", "end": "2026-08-31" }),
        );
        assert_eq!(
            (today["matches"].as_u64(), today["searchedFrom"].as_str()),
            (Some(3), Some("2025-08-30"))
        );
        let day = ledger.call(
            "searchTransactions",
            &json!({ "start": "2026-08-12", "end": "2026-08-12" }),
        );
        assert!(day.get("searchedFrom").is_none());
        assert_eq!(search["transactions"][0]["date"], "2026-08-12");
        let accounts = ledger.call("listAccounts", &json!({}));
        assert!(accounts["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| account["name"] == "Everyday"));
        let history = ledger.call("merchantHistory", &json!({ "merchant": "netflix" }));
        assert_eq!(history["transactions"], 3);
        assert_eq!(history["netAmount"], -46.47);
        assert!(ledger.call("merchantHistory", &json!({}))["error"].is_string());
        assert!(ledger.call(
            "cashFlow",
            &json!({ "start": "2026-08-31", "end": "2026-08-01" })
        )["error"]
            .is_string());
        assert!(ledger.call("deleteEverything", &json!({}))["error"].is_string());
    }

    #[test]
    fn insight_tools_quote_native_results() {
        let view = view(true);
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        let ledger = Ledger::new(&view, today);
        let cards = ledger.call("creditCards", &json!({}));
        assert_eq!(cards["cards"][1]["card"], "Freedom Unlimited");
        assert_eq!(cards["cards"][1]["utilizationPercent"], 30.67);
        let taxes = ledger.call("taxSummary", &json!({}));
        assert_eq!(
            (taxes["year"].as_i64(), taxes["interest"].as_f64()),
            (Some(2026), Some(18.42))
        );
        assert_eq!(taxes["openWashSaleWindows"][0]["ticker"], "AAPL");
        assert!(taxes["estimatedTaxHigh"].is_null());
        assert_eq!(
            ledger.call("trips", &json!({}))["trips"][0]["destination"],
            "Austin, TX"
        );
        let subscriptions = ledger.call("subscriptions", &json!({}));
        assert!(subscriptions["subscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "Spotify" && item["previousAmount"] == -11.99));
        assert_eq!(
            ledger.call("dividendIncome", &json!({}))["upcoming"][0]["ticker"],
            "MSFT"
        );
    }
}
