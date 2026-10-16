//! Native financial insights derived from the annotation-aware committed snapshot: expected
//! activity, statement cycles, card benefits, and plan. Rust is the only policy;
//! the renderer formats these results and never recomputes them.
pub mod agenda;
pub mod allocation;
pub mod anomalies;
pub mod attention;
pub mod benefits;
pub mod calendar;
pub mod cashline;
pub mod changes;
pub mod credit;
pub mod dividends;
pub mod documents;
pub mod forecast;
pub mod interest;
pub mod investing;
pub mod ledger;
pub mod lots;
pub mod people;
pub mod plan;
pub mod recap;
pub mod reconcile;
pub mod recurring;
pub mod refunds;
pub mod retirement;
pub mod returns;
pub mod spending;
pub mod statement;
pub mod subscriptions;
pub mod tax;
pub mod totals;
pub mod trips;

#[cfg(test)]
mod golden;

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::finance_contract::{Account, Snapshot, Transaction};
use crate::numeric::{from_minor, minor_units};
use crate::settings::FinanceSettings;
use benefits::{
    BenefitActivity, BenefitHistory, BenefitOption, BenefitReview, BenefitWindow, PLATINUM,
};
use calendar::Zone;

/// A snapshot as it is read: every saved edit applied, its insights when they could be derived,
/// and the names the user gave accounts. Rust's readers share it; the renderer gets its JSON.
pub struct View {
    pub snapshot: Snapshot,
    pub insights: Option<Insights>,
    pub account_names: BTreeMap<String, String>,
}

impl View {
    /// The name the user gave an account, or else its provider's name.
    pub fn account_name(&self, id: &str) -> Option<&str> {
        self.account_names.get(id).map(String::as_str).or_else(|| {
            self.snapshot
                .accounts
                .iter()
                .find(|account| account.id == id)
                .map(|account| account.name.as_str())
        })
    }
}

#[cfg(test)]
impl View {
    /// A JSON snapshot as read, without edits, insights, or account names.
    pub fn of(snapshot: serde_json::Value) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            snapshot: serde_json::from_value(snapshot).unwrap(),
            insights: None,
            account_names: BTreeMap::new(),
        })
    }
}

/// The account fields insights read. Missing values stay unavailable.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AccountView {
    pub id: String,
    pub activity_as_of: Option<String>,
    pub balance_fetched_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardBenefits {
    pub account_id: String,
    /// The card product whose rule data produced these results.
    pub product: String,
    pub product_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<benefits::FeeRecovery>,
    pub activity: Vec<BenefitActivity>,
    /// Current windows as of now, including hidden benefits; the renderer filters them.
    pub windows: Vec<BenefitWindow>,
    /// The windows with allowance left, by ID, in the order to use them.
    pub use_first: Vec<String>,
    /// Each calendar benefit's periods from the history's first year through this one,
    /// including hidden benefits; the renderer filters them.
    pub history: Vec<BenefitHistory>,
    /// Matches that need a decision, including hidden benefits'.
    pub review: Vec<BenefitReview>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Insights {
    pub today: NaiveDate,
    pub computed_at: String,
    pub expected: Vec<forecast::ExpectedEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement: Option<statement::StatementCycle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benefits: Option<CardBenefits>,
    /// Benefits for other credit cards the user assigned a product to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub other_card_benefits: Vec<CardBenefits>,
    /// Card products with rule data, for assigning one to a card.
    pub benefit_products: Vec<BenefitOption>,
    pub benefit_options: Vec<BenefitOption>,
    pub benefit_rules_version: String,
    pub benefit_terms_reviewed: String,
    /// Unlinked refunds and the one purchase each most plausibly repays.
    pub refund_suggestions: Vec<refunds::RefundSuggestion>,
    pub plan: plan::Plan,
    pub attention: Vec<attention::AttentionItem>,
    pub brief: attention::Brief,
    pub cash: cashline::CashTimeline,
    pub subscriptions: subscriptions::Subscriptions,
    /// Card utilization and the interest charged on the last statements.
    pub credit: credit::Credit,
    /// Possible duplicate charges and unusual amounts, not yet reviewed.
    pub anomalies: anomalies::Anomalies,
    pub trips: trips::Trips,
    /// Who transactions and trips were with.
    pub people: people::People,
    /// Plaid's recurring streams against Brief's forecast, when Recurring Transactions is on.
    pub recurring_check: recurring::RecurringCheck,
    /// Spending's scopes and their periods.
    pub spending: Vec<spending::SpendingScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounts_total: Option<totals::AccountsTotal>,
    pub returns: Vec<returns::AccountReturns>,
    /// The combined brokerages' statement for each chart window: the identity, two labeled
    /// returns beside the benchmark, the waterfall, and risk.
    pub periods: Vec<reconcile::Statement>,
    /// How concentrated the combined brokerages' holdings are now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure: Option<reconcile::Exposure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocation: Option<allocation::Allocation>,
    /// Every security held, merged across accounts, for the holdings list.
    pub securities: Vec<allocation::SecurityHolding>,
    /// What happened over each chart period.
    pub recap: Vec<recap::PeriodRecap>,
    /// Contributions against this year's limits, tax buckets, savings rate, and coverage.
    pub retirement: retirement::Retirement,
    /// Pay statements matched to deposits, each tax year's expected forms, W-2 predictions,
    /// and how long to keep each document.
    pub documents: documents::Documents,
    pub lots: Vec<lots::PositionLots>,
    /// Lots turning long-term soon, so a sale can wait for the lower rate.
    pub long_term_soon: Vec<lots::LongTermSoon>,
    /// What about the investments calls for a decision.
    pub investment_attention: investing::InvestmentAttention,
    /// Account balance changes and posted activity for each chart period.
    pub changes: Vec<changes::PeriodChange>,
    /// Each cash account's interest by chart period and its monthly net flow.
    pub interest: Vec<interest::AccountInterest>,
    /// This year's and last year's taxable investment income, sales, and estimates.
    pub taxes: tax::Taxes,
    /// Dividend pace, yield, and announced payments for the securities held.
    pub dividends: dividends::DividendOutlook,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movers: Option<lots::Movers>,
    /// Dated items, for the Calendar page and the Brief calendar.
    pub agenda: Vec<agenda::AgendaItem>,
}

/// Derives insights for an annotation-aware snapshot. The snapshot takes the saved tax
/// treatments, the card credits that follow a purchase made for someone else, and each
/// transaction's recurring series identity. `now` and `zone` define the user's calendar day.
#[cfg(test)]
pub fn derive(
    snapshot: &mut Snapshot,
    settings: &FinanceSettings,
    now: DateTime<Utc>,
    zone: &dyn Zone,
) -> Result<Insights, String> {
    derive_with_documents(snapshot, settings, &[], now, zone)
}

/// `derive`, with what Brief read from the user's documents.
pub fn derive_with_documents(
    snapshot: &mut Snapshot,
    settings: &FinanceSettings,
    documents: &[crate::documents::Document],
    now: DateTime<Utc>,
    zone: &dyn Zone,
) -> Result<Insights, String> {
    for account in &mut snapshot.accounts {
        if let Some(treatment) = settings.account_tax_treatments.get(&account.id) {
            account.tax_treatment = Some(treatment.clone());
        }
    }
    credits_follow_purchases(
        &mut snapshot.transactions,
        &snapshot.accounts,
        settings,
        &snapshot.updated_at,
    )?;
    for transaction in &mut snapshot.transactions {
        transaction.recurring_id = Some(forecast::recurring_key(transaction));
    }
    people::assign(&mut snapshot.transactions, &settings.people);
    let snapshot = &*snapshot;
    let (transactions, accounts, updated_at) = (
        &snapshot.transactions,
        &snapshot.accounts,
        &snapshot.updated_at,
    );
    let ended = recurring::ended(&snapshot.recurring_streams, transactions);
    let Cashflow {
        today,
        expected,
        statement,
        benefits,
        other_card_benefits,
        refund_suggestions,
        plan,
        attention,
        mut brief,
        cash,
        subscriptions,
        credit,
        anomalies,
        mut trips,
        horizon,
    } = cashflow(
        transactions,
        accounts,
        settings,
        updated_at,
        now,
        zone,
        &ended,
    );
    people::trip_companions(
        &mut trips.trips,
        &settings.trip_companions,
        &settings.people,
    );
    let people = people::people(transactions, &settings.people, &trips.trips);
    let recurring_check = recurring::check(&snapshot.recurring_streams, &expected, transactions);
    // Statements belong to the saved card, and only once its benefits say it is that card.
    let card = spending_card(accounts, settings);
    let card_statement = statement.as_ref().filter(|_| {
        benefits
            .as_ref()
            .zip(card)
            .is_some_and(|(benefits, card)| benefits.account_id == card.id)
    });
    let spending = spending::scopes(
        transactions,
        accounts,
        card,
        card_statement,
        updated_at,
        zone,
    );
    let (performance, holdings, trades) = (
        &snapshot.brokerage_performance,
        &snapshot.holdings,
        &snapshot.trades,
    );
    let allocation = allocation::allocation(holdings, accounts);
    let securities =
        allocation::security_holdings(holdings, allocation.as_ref().map(|value| value.total));
    let mut lots = lots::lots(holdings, trades, today);
    for position in &mut lots {
        position.tax_deferred = accounts
            .iter()
            .any(|account| account.id == position.account_id && !account.taxable());
    }
    let long_term_soon = lots::long_term_soon(&lots, today);
    let investment_attention = investing::attention(
        allocation.as_ref(),
        &securities,
        &long_term_soon,
        holdings,
        accounts,
    );
    let harvestable = investment_attention
        .flags
        .iter()
        .filter_map(|flag| match flag {
            investing::InvestmentFlag::HarvestableLoss {
                ticker, account_id, ..
            } => Some((ticker.clone(), account_id.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let taxes = tax::taxes(
        tax::TaxInputs {
            transactions,
            trades,
            accounts,
            positions: &lots,
            harvestable: &harvestable,
            profile: settings.tax_profile.as_ref(),
        },
        today,
    );
    if let Some(rates) = tax::rates(&taxes) {
        for position in lots.iter_mut().filter(|position| !position.tax_deferred) {
            for lot in &mut position.lots {
                let rate = if lot.long_term {
                    rates.long_term
                } else {
                    rates.short_term
                };
                lot.estimated_tax = lot
                    .gain
                    .map(|gain| from_minor(minor_units(gain * rate / 100.0)));
            }
        }
    }
    brief.needs.extend(
        taxes
            .wash_windows
            .iter()
            .map(|window| attention::AttentionItem {
                id: format!("wash-window:{}", window.sale_id),
                severity: attention::Severity::Attention,
                item: attention::Item::WashWindow {
                    ticker: window.ticker.clone(),
                    account_id: window.account_id.clone(),
                    sold_on: window.sold_on,
                    until: window.until,
                    loss: window.loss,
                },
            }),
    );
    // A due bill or deposit that never came, unless its series is ended or its last
    // transaction was marked reviewed, which is how the reminder is dismissed. A subscription
    // that stops charging was most likely cancelled, so it asks for nothing.
    let views = accounts
        .iter()
        .map(|account| AccountView {
            id: account.id.clone(),
            activity_as_of: account.activity_as_of.clone(),
            balance_fetched_at: account.balance_fetched_at.clone(),
        })
        .collect::<Vec<_>>();
    let reviewed = |id: &str| {
        transactions
            .iter()
            .any(|transaction| transaction.id == id && transaction.reviewed == Some(true))
    };
    brief.needs.extend(
        forecast::missed_events(transactions, &views, updated_at, today)
            .into_iter()
            .filter(|event| {
                event.kind != forecast::EventKind::Subscription
                    && !ended.contains(&event.recurring_id)
                    && !reviewed(&event.source_transaction_id)
            })
            .map(|event| attention::AttentionItem {
                id: format!("missed:{}:{}", event.recurring_id, event.date),
                severity: attention::Severity::Attention,
                item: attention::Item::MissedBill {
                    recurring_id: event.recurring_id,
                    title: event.title,
                    account_id: event.account_id,
                    expected: event.date,
                    days: (today - event.date).num_days(),
                    amount: event.amount,
                    event: event.kind,
                    source_transaction_id: event.source_transaction_id,
                },
            }),
    );
    let interest = interest::interest(accounts, transactions, today);
    brief.needs.extend(
        interest::idle_cash(accounts, &interest)
            .into_iter()
            .map(|idle| attention::AttentionItem {
                id: format!("idle-cash:{}", idle.account_id),
                severity: attention::Severity::Attention,
                item: attention::Item::IdleCash {
                    account_id: idle.account_id,
                    balance: idle.balance,
                    annual_rate: idle.annual_rate,
                    better_account_id: idle.better_account_id,
                    better_rate: idle.better_rate,
                },
            }),
    );
    let recap = recap::recap(transactions, today);
    let retirement = retirement::retirement(
        retirement::Inputs {
            transactions,
            accounts,
            settings,
            recap: &recap,
            stated: &snapshot.contribution_years,
            documents,
        },
        today,
    );
    brief.needs.extend(retirement::needs(&retirement));
    let document_insights = documents::documents(
        documents::Inputs {
            documents,
            transactions,
            accounts,
            filing_status: settings
                .tax_profile
                .as_ref()
                .map(|profile| profile.filing_status.as_str()),
        },
        today,
    );
    brief
        .needs
        .extend(documents::needs(&document_insights, documents, today));
    brief.needs.sort_by_key(|item| item.severity);
    let mut insights = Insights {
        today,
        computed_at: now.to_rfc3339(),
        expected,
        statement,
        benefits,
        other_card_benefits,
        benefit_products: benefits::products()
            .into_iter()
            .map(|rules| BenefitOption {
                id: rules.product.clone(),
                name: rules.name.clone(),
            })
            .collect(),
        benefit_options: PLATINUM.options(),
        benefit_rules_version: PLATINUM.version.clone(),
        benefit_terms_reviewed: PLATINUM.terms_reviewed.clone(),
        refund_suggestions,
        plan,
        attention,
        brief,
        cash,
        subscriptions,
        credit,
        anomalies,
        trips,
        people,
        recurring_check,
        spending,
        accounts_total: {
            let cards = accounts
                .iter()
                .filter(|account| {
                    account.r#type == "credit"
                        && settings.spending_account_ids().any(|id| id == account.id)
                })
                .collect::<Vec<_>>();
            (!cards.is_empty())
                .then(|| totals::accounts_total(snapshot, &cards))
                .flatten()
        },
        returns: returns::returns(performance),
        periods: reconcile::statements(performance, trades, transactions),
        exposure: reconcile::exposure(performance, holdings),
        allocation,
        securities,
        recap,
        retirement,
        documents: document_insights,
        lots,
        long_term_soon,
        investment_attention,
        changes: changes::changes(
            &snapshot.account_balance_history,
            performance,
            transactions,
            today,
        ),
        interest,
        taxes,
        // A snapshot saved without dividend history has no outlook, not an empty one.
        dividends: if snapshot.dividend_history.is_empty() {
            dividends::DividendOutlook::default()
        } else {
            dividends::outlook(&snapshot.dividend_history, holdings, today)
        },
        movers: lots::movers(holdings),
        agenda: Vec::new(),
    };
    insights.agenda = agenda::agenda(agenda::Inputs {
        today,
        accounts,
        forecast: &horizon,
        credit: &insights.credit,
        benefits: insights
            .benefits
            .iter()
            .chain(&insights.other_card_benefits)
            .map(|card| (card.account_id.as_str(), card.windows.as_slice()))
            .collect(),
        hidden_benefit_ids: &settings.hidden_benefit_ids,
        dividends: &insights.dividends.upcoming,
        long_term_soon: &insights.long_term_soon,
        wash_windows: &insights.taxes.wash_windows,
        trips: &insights.trips.trips,
    });
    Ok(insights)
}

/// `derive` over a JSON snapshot, which takes the derived fields and its insights, for tests
/// that build and read snapshots as JSON.
#[cfg(test)]
pub fn attach(
    snapshot: &mut serde_json::Value,
    settings: &FinanceSettings,
    now: DateTime<Utc>,
    zone: &dyn Zone,
) -> Result<(), String> {
    let mut typed: Snapshot =
        serde_json::from_value(snapshot.clone()).map_err(|error| error.to_string())?;
    let insights = derive(&mut typed, settings, now, zone)?;
    *snapshot = serde_json::to_value(&typed).map_err(|error| error.to_string())?;
    snapshot["insights"] = serde_json::to_value(insights).map_err(|error| error.to_string())?;
    Ok(())
}

/// What the cash accounts and cards say: expected activity, statements, card benefits, the
/// plan, and what needs attention.
struct Cashflow {
    today: NaiveDate,
    expected: Vec<forecast::ExpectedEvent>,
    statement: Option<statement::StatementCycle>,
    benefits: Option<CardBenefits>,
    other_card_benefits: Vec<CardBenefits>,
    refund_suggestions: Vec<refunds::RefundSuggestion>,
    plan: plan::Plan,
    attention: Vec<attention::AttentionItem>,
    brief: attention::Brief,
    cash: cashline::CashTimeline,
    subscriptions: subscriptions::Subscriptions,
    credit: credit::Credit,
    anomalies: anomalies::Anomalies,
    trips: trips::Trips,
    /// The forecast over the cash horizon, with each series' later occurrences.
    horizon: Vec<forecast::ExpectedEvent>,
}

#[cfg(test)]
fn compute(
    transactions: &[Transaction],
    accounts: &[Account],
    settings: &FinanceSettings,
    updated_at: &str,
    now: DateTime<Utc>,
    zone: &dyn Zone,
) -> Cashflow {
    cashflow(
        transactions,
        accounts,
        settings,
        updated_at,
        now,
        zone,
        &std::collections::BTreeSet::new(),
    )
}

/// `ended` holds recurring series Plaid reports cancelled; they are not forecast.
fn cashflow(
    transactions: &[Transaction],
    accounts: &[Account],
    settings: &FinanceSettings,
    updated_at: &str,
    now: DateTime<Utc>,
    zone: &dyn Zone,
    ended: &std::collections::BTreeSet<String>,
) -> Cashflow {
    let today = zone.date(now);
    let live = |mut events: Vec<forecast::ExpectedEvent>| {
        events.retain(|event| !ended.contains(&event.recurring_id));
        events
    };
    let views = accounts
        .iter()
        .map(|account| AccountView {
            id: account.id.clone(),
            activity_as_of: account.activity_as_of.clone(),
            balance_fetched_at: account.balance_fetched_at.clone(),
        })
        .collect::<Vec<_>>();
    let expected = live(forecast::expected_events(
        transactions,
        &views,
        updated_at,
        today,
    ));
    let card = spending_card(accounts, settings);
    let card_transactions = card
        .map(|card| {
            transactions
                .iter()
                .filter(|transaction| transaction.account_id.as_deref() == Some(card.id.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let statement = card
        .and_then(|card| statement::cycle(&card_transactions, updated_at, card.liability.as_ref()));
    let now_iso = now.to_rfc3339();
    // Charges recurring series should make through December, for planning benefit credits.
    let year_end = NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap_or(today);
    let upcoming = live(forecast::forecast_within(
        transactions,
        &views,
        updated_at,
        today,
        (year_end - today).num_days().max(0),
    ));
    // Benefits for one card under one product's rules.
    let card_benefits = |card: &Account, rules: &'static benefits::RuleSet| {
        let own = transactions
            .iter()
            .filter(|transaction| transaction.account_id.as_deref() == Some(card.id.as_str()))
            .collect::<Vec<_>>();
        let activity = rules.activity(&own, updated_at);
        let history_start = own
            .iter()
            .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
            .min();
        let windows = rules.tracker_for(&activity, &now_iso, updated_at, history_start, zone);
        // A series counts toward the benefit its latest charge qualifies for.
        let charges = upcoming
            .iter()
            .filter(|event| event.amount < 0.0)
            .filter_map(|event| {
                let source = own
                    .iter()
                    .find(|transaction| transaction.id == event.source_transaction_id)?;
                let benefit_id = rules.purchase_benefit(source)?;
                Some(
                    std::iter::once(event.date)
                        .chain(event.later.iter().copied())
                        .map(move |date| benefits::PlannedCharge {
                            benefit_id: benefit_id.clone(),
                            date,
                            amount: event.amount,
                            title: event.title.clone(),
                        }),
                )
            })
            .flatten()
            .collect::<Vec<_>>();
        let history = rules.history(&activity, history_start, today, &charges);
        CardBenefits {
            use_first: benefits::use_first(&windows),
            account_id: card.id.clone(),
            product: rules.product.clone(),
            product_name: rules.name.clone(),
            fee: rules.fee_recovery(&own, &activity, &history, today),
            windows,
            history,
            review: rules.review(&own, &activity),
            activity,
        }
    };
    let tracked = tracked_cards(accounts, settings);
    let is_spending_card = |account: &Account| card.is_some_and(|card| card.id == account.id);
    let benefits = tracked
        .iter()
        .find(|(account, _)| is_spending_card(account))
        .map(|(account, rules)| card_benefits(account, rules));
    let other_card_benefits = tracked
        .iter()
        .filter(|(account, _)| !is_spending_card(account))
        .map(|(account, rules)| card_benefits(account, rules))
        .collect::<Vec<_>>();
    let plan = plan::plan(transactions, accounts, &expected, today);
    let context = attention::Context {
        today,
        spending_account_id: card.map(|card| card.id.as_str()),
        statement: statement.as_ref(),
        plan: &plan,
    };
    let attention = attention::attention(context);
    let horizon = live(forecast::forecast_within(
        transactions,
        &views,
        updated_at,
        today,
        cashline::HORIZON_DAYS,
    ));
    let cash = cashline::timeline(accounts, &horizon, today);
    let subscriptions = subscriptions::subscriptions(
        &live(forecast::forecast_within(
            transactions,
            &views,
            updated_at,
            today,
            subscriptions::HORIZON_DAYS,
        )),
        transactions,
    );
    let credit = credit::credit(
        accounts,
        card.zip(statement.as_ref())
            .map(|(card, cycle)| (card.id.as_str(), cycle)),
        today,
    );
    let anomalies = anomalies::anomalies(transactions, today);
    let brief = attention::Brief {
        needs: attention::needs(
            context,
            &benefits
                .iter()
                .chain(&other_card_benefits)
                .map(|card| {
                    (
                        card.account_id.as_str(),
                        card.windows.as_slice(),
                        card.review.as_slice(),
                    )
                })
                .collect::<Vec<_>>(),
            &settings.hidden_benefit_ids,
            &cash,
            &credit,
            &anomalies,
        ),
    };
    Cashflow {
        today,
        expected,
        statement,
        benefits,
        other_card_benefits,
        refund_suggestions: refunds::suggestions(transactions, today),
        plan,
        attention,
        brief,
        cash,
        subscriptions,
        credit,
        anomalies,
        trips: trips::trips(transactions, today),
        horizon,
    }
}

/// Credit cards whose benefits are tracked, with their product's rules: the spending card under
/// its assigned product or the default one, and any other card only once assigned a product.
fn tracked_cards<'a>(
    accounts: &'a [Account],
    settings: &FinanceSettings,
) -> Vec<(&'a Account, &'static benefits::RuleSet)> {
    let card = spending_card(accounts, settings);
    accounts
        .iter()
        .filter(|account| account.r#type == "credit")
        .filter_map(|account| {
            let assigned = settings
                .benefit_cards
                .get(&account.id)
                .and_then(|product| benefits::product(product));
            let spending = card.is_some_and(|card| card.id == account.id);
            let rules = if spending {
                assigned.or_else(|| benefits::product(benefits::DEFAULT_PRODUCT))
            } else {
                assigned
            };
            Some((account, rules?))
        })
        .collect()
}

/// A card credit belongs to whoever its purchase was for: a credit paired with a purchase made
/// for someone else, and a reversal of that credit, are someone else's money too.
fn credits_follow_purchases(
    transactions: &mut [Transaction],
    accounts: &[Account],
    settings: &FinanceSettings,
    updated_at: &str,
) -> Result<(), String> {
    let mut others = transactions
        .iter()
        .filter(|transaction| transaction.other)
        .map(|transaction| transaction.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut follow = Vec::new();
    for (card, rules) in tracked_cards(accounts, settings) {
        let own = transactions
            .iter()
            .filter(|transaction| transaction.account_id.as_deref() == Some(card.id.as_str()))
            .collect::<Vec<_>>();
        let activity = rules.activity(&own, updated_at);
        // Credits first, so a reversal can follow the credit it undoes.
        for kind in [
            benefits::ActivityKind::Credit,
            benefits::ActivityKind::Reversal,
        ] {
            for item in activity.iter().filter(|item| item.kind == kind) {
                let paired = item
                    .paired_id
                    .as_deref()
                    .is_some_and(|id| others.contains(id));
                if paired && others.insert(item.id.clone()) {
                    follow.push(item.id.clone());
                }
            }
        }
    }
    for transaction in transactions.iter_mut() {
        if !follow.contains(&transaction.id) {
            continue;
        }
        let classification = transaction
            .classification
            .as_mut()
            .ok_or("Unclassified transaction")?;
        crate::transaction_policy::owe(classification, transaction.amount);
        transaction.other = true;
    }
    Ok(())
}

/// Spending and benefits use only the explicitly saved credit account, never ordering.
fn spending_card<'a>(accounts: &'a [Account], settings: &FinanceSettings) -> Option<&'a Account> {
    settings.spending_account_id.as_deref().and_then(|id| {
        accounts
            .iter()
            .find(|account| account.id == id && account.r#type == "credit")
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod seed_fixture;
