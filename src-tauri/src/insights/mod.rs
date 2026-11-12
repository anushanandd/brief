//! Native financial insights derived from the annotation-aware committed snapshot: expected
//! activity, statement cycles, card benefits, and plan. Rust is the only policy;
//! the renderer formats these results and never recomputes them.
pub mod activity_totals;
pub mod agenda;
pub mod allocation;
pub mod anomalies;
pub mod assistant_planner;
pub mod assistant_tools;
pub mod attention;
pub mod benefits;
pub mod calendar;
pub mod cashline;
pub mod chart_change;
pub mod credit;
pub mod dividends;
pub mod documents;
pub mod flows;
pub mod forecast;
pub mod interest;
pub mod investing;
pub mod lots;
pub mod people;
pub mod period_changes;
pub mod plan;
pub mod precision;
pub mod recap;
pub mod reconcile;
pub mod recurring;
pub mod refunds;
pub mod reports;
pub mod retirement;
pub mod returns;
pub mod rollup;
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
use ts_rs::TS;

use crate::finance_contract::{Account, Snapshot, Transaction};
use crate::numeric::{from_minor, minor_units};
use crate::settings::FinanceSettings;
use benefits::{BenefitActivity, BenefitHistory, BenefitOption, BenefitReview, BenefitWindow};
use calendar::Zone;

/// A snapshot as it is read: every saved edit applied, its insights when they could be derived,
/// and the names the user gave accounts. Rust's readers share it; the renderer gets its JSON.
pub struct View {
    pub snapshot: Snapshot,
    pub insights: Option<Insights>,
    pub account_names: BTreeMap<String, String>,
    /// The saved spending card, whose statements and benefits Spending and Benefits read.
    pub spending_account_id: Option<String>,
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
            spending_account_id: None,
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

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CardBenefits {
    pub account_id: String,
    /// The card product whose rule data produced these results.
    pub product: String,
    pub product_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fee: Option<benefits::FeeRecovery>,
    pub activity: Vec<BenefitActivity>,
    /// Current windows as of now, including hidden benefits; the renderer filters them.
    pub windows: Vec<BenefitWindow>,
    /// The windows with allowance left, by ID, in the order to use them.
    pub use_first: Vec<String>,
    /// Each calendar benefit's periods from the history's first year through this one,
    /// including hidden benefits; the renderer filters them.
    pub history: Vec<BenefitHistory>,
    /// Each year's periods summed over the benefits shown, oldest first.
    pub years: Vec<benefits::CardBenefitYear>,
    /// Matches that need a decision, including hidden benefits'.
    pub review: Vec<BenefitReview>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Insights {
    pub today: NaiveDate,
    pub computed_at: String,
    pub expected: Vec<forecast::ExpectedEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub statement: Option<statement::StatementCycle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub benefits: Option<CardBenefits>,
    /// Benefits for other credit cards the user assigned a product to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_card_benefits: Vec<CardBenefits>,
    /// Money from elsewhere filed as repaying card fees, every tracked card's `fee.offsets`
    /// together, in whole cents; none when no tracked card has a fee year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fee_offsets: Option<f64>,
    /// Card products with saved rules, for assigning one to a card.
    pub benefit_products: Vec<BenefitOption>,
    /// The spending card's benefits, and its product's rules version and review date; none
    /// when no product's rules cover it.
    pub benefit_options: Vec<BenefitOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub benefit_rules_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub benefit_terms_reviewed: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accounts_total: Option<totals::AccountsTotal>,
    /// Home's total by day, without the spending cards when there are any, split into cash,
    /// securities, and the rest, with its drawdown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub net_worth_parts: Vec<totals::NetWorthPart>,
    /// Each account group's value and daily history, the spending cards as their own group.
    pub group_history: Vec<totals::GroupHistory>,
    /// Account totals from saved values; each market projection carries its own from live ones.
    pub rollup: rollup::Rollup,
    /// Each value chart's change over each window at its saved values; the market stream sends
    /// its own at live ones.
    pub chart_changes: Vec<chart_change::ChartChanges>,
    /// What moved through each account over each chart period.
    pub flows: Vec<flows::AccountFlows>,
    /// Each day's and month's spending and income across every account, for the calendar.
    pub activity_days: Vec<flows::Totals>,
    pub activity_months: Vec<flows::Totals>,
    pub returns: Vec<returns::AccountReturns>,
    /// The combined brokerages' statement for each chart window: the identity, two labeled
    /// returns beside the benchmark, the waterfall, and risk.
    pub periods: Vec<reconcile::Statement>,
    /// How concentrated the combined brokerages' holdings are now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exposure: Option<reconcile::Exposure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
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
    pub changes: Vec<period_changes::PeriodChange>,
    /// Each cash account's interest by chart period and its monthly net flow.
    pub interest: Vec<interest::AccountInterest>,
    /// This year's and last year's taxable investment income, sales, and estimates.
    pub taxes: tax::Taxes,
    /// Dividend pace, yield, and announced payments for the securities held.
    pub dividends: dividends::DividendOutlook,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub movers: Option<lots::Movers>,
    /// Dated items, for the Calendar page and the Brief calendar.
    pub agenda: Vec<agenda::AgendaItem>,
    /// What program rules explain or suggest, with proofs, and life chapters.
    pub evidence: crate::evidence::Evidence,
    /// Each detector's share of your answers that accepted it, for Diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub precision: Vec<precision::DetectorPrecision>,
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
    // Rules run before any total, so an Explained offset already counts as a refund.
    let explained = crate::evidence::explain(
        &snapshot.transactions,
        &snapshot.accounts,
        documents,
        settings,
        zone.date(now),
    );
    crate::evidence::apply(&mut snapshot.transactions, &explained.explanations);
    let snapshot = &*snapshot;
    let (transactions, accounts, updated_at) = (
        &snapshot.transactions,
        &snapshot.accounts,
        &snapshot.updated_at,
    );
    let ended = recurring::ended(&snapshot.recurring_streams, transactions);
    let today = zone.date(now);
    let mut trips = trips::trips(transactions, today);
    // Purchases on trips say nothing about where home is.
    let away = trips
        .trips
        .iter()
        .flat_map(|trip| trip.transaction_ids.iter().cloned())
        .collect();
    let evidence =
        crate::evidence::evidence(explained, transactions, accounts, &away, settings, today);
    people::trip_companions(
        &mut trips.trips,
        &settings.trip_companions,
        &settings.people,
    );
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
        trips,
        horizon,
    } = cashflow(
        transactions,
        accounts,
        settings,
        updated_at,
        now,
        zone,
        &ended,
        trips,
        crate::evidence::chapters::baseline(&evidence.chapters, today),
    );
    let people = people::people(transactions, &settings.people, &trips.trips);
    // A month that already fell short can't be changed, so its watch note stays with the rule;
    // an ended job or a missing bonus still asks something of you.
    brief.needs.extend(
        evidence
            .alerts
            .iter()
            .filter(|alert| alert.kind != crate::evidence::AlertKind::Watch)
            .map(|alert| attention::AttentionItem {
                id: format!("program:{}", alert.id),
                severity: attention::Severity::Attention,
                item: attention::Item::ProgramNote {
                    program: alert.program.clone(),
                    says: alert.says.clone(),
                    subject: alert.subject.clone(),
                },
            }),
    );
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
    let mut taxes = tax::taxes(
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
    // Pay statements project this year's withholding; last year's W-2s supply the prior-year
    // test once all of them are in.
    let prior_wages = document_insights
        .years
        .iter()
        .find(|year| year.year == today.year() - 1)
        .and_then(|year| year.income.as_ref())
        .filter(|income| income.wages_from == "w2" && income.self_employment == 0.0)
        .map(|income| income.wages);
    taxes.withholding = tax::withholding(
        today.year(),
        documents::pay_projections(today.year(), documents),
        taxes.years.iter().find(|year| year.year == today.year()),
        prior_wages,
        tax::prior_investment(&taxes, transactions, trades, today),
        settings
            .tax_profile
            .as_ref()
            .map(|profile| profile.filing_status.as_str()),
    );
    brief.needs.sort_by_key(|item| item.severity);
    // The spending cards stay out of the totals and their history.
    let cards = rollup::spending_cards(accounts, settings);
    let accounts_total = (!cards.is_empty())
        .then(|| totals::accounts_total(snapshot, &cards))
        .flatten();
    let card_ids = cards
        .iter()
        .map(|card| card.id.as_str())
        .collect::<Vec<_>>();
    let rollup = rollup::rollup(accounts, holdings, snapshot.net_worth, settings);
    let group_history = totals::group_histories(snapshot, settings, &rollup);
    let net_worth_parts = totals::net_worth_parts(
        snapshot,
        accounts_total
            .as_ref()
            .map_or(&snapshot.net_worth_history, |total| &total.history),
    );
    let (activity_days, activity_months) = flows::calendar_totals(snapshot);
    let chart_changes = chart_change::changes(
        &chart_change::series(
            snapshot,
            settings,
            accounts_total
                .as_ref()
                .map_or(&snapshot.net_worth_history, |total| &total.history),
            rollup.net_worth,
            &group_history,
        ),
        &BTreeMap::new(),
        now.timestamp(),
    );
    let fee_offsets = benefits
        .iter()
        .chain(&other_card_benefits)
        .filter_map(|card| card.fee.as_ref())
        .map(|fee| minor_units(fee.offsets))
        .reduce(|total, offsets| total + offsets)
        .map(from_minor);
    let cards = benefits::Catalog::from_settings(settings);
    let spending_rules = benefits
        .as_ref()
        .and_then(|card| cards.product(&card.product));
    let benefit_options = spending_rules
        .map(|rules| {
            let mut options = rules.options();
            for option in &mut options {
                if let Some(item) = benefits
                    .as_ref()
                    .and_then(|card| card.history.iter().find(|item| item.id == option.id))
                {
                    option.logo_domain = item.logo_domain.clone();
                    option.logo_url = item.logo_url.clone();
                    option.logo_brand = item.logo_brand.clone();
                }
            }
            options
        })
        .unwrap_or_default();
    let mut insights = Insights {
        today,
        computed_at: now.to_rfc3339(),
        expected,
        statement,
        benefits,
        other_card_benefits,
        fee_offsets,
        benefit_products: cards
            .products()
            .iter()
            .map(|rules| BenefitOption {
                id: rules.product.clone(),
                name: rules.name.clone(),
                logo_domain: None,
                logo_url: None,
                logo_brand: None,
                category: None,
            })
            .collect(),
        benefit_options,
        benefit_rules_version: spending_rules.map(|rules| rules.version.clone()),
        benefit_terms_reviewed: spending_rules.map(|rules| rules.terms_reviewed.clone()),
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
        accounts_total,
        net_worth_parts,
        group_history,
        rollup,
        chart_changes,
        flows: flows::flows(snapshot),
        activity_days,
        activity_months,
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
        changes: period_changes::changes(
            &snapshot.account_balance_history,
            performance,
            transactions,
            accounts,
            &card_ids,
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
        evidence,
        precision: precision::precision(settings, transactions),
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
        trips::trips(transactions, zone.date(now)),
        None,
    )
}

/// `ended` holds recurring series Plaid reports cancelled; they are not forecast. `baseline`
/// is where a confirmed chapter starts a new normal for typical spending and usual amounts.
#[allow(clippy::too_many_arguments)]
fn cashflow(
    transactions: &[Transaction],
    accounts: &[Account],
    settings: &FinanceSettings,
    updated_at: &str,
    now: DateTime<Utc>,
    zone: &dyn Zone,
    ended: &std::collections::BTreeSet<String>,
    trips: trips::Trips,
    baseline: Option<NaiveDate>,
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
    let card_benefits = |card: &Account, rules: &benefits::RuleSet| {
        let own = transactions
            .iter()
            .filter(|transaction| transaction.account_id.as_deref() == Some(card.id.as_str()))
            .collect::<Vec<_>>();
        let rules = &rules.for_card(&own);
        let activity = rules.activity(&own, updated_at);
        let history_start = own
            .iter()
            .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
            .min();
        let logos = rules.logos(&own, &activity);
        let mut windows = rules.tracker_for(&activity, &now_iso, updated_at, history_start, zone);
        for window in &mut windows {
            let logo = logos.get(&window.id).cloned().unwrap_or_default();
            window.logo_domain = logo.domain;
            window.logo_url = logo.url;
            window.logo_brand = logo.brand;
        }
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
        let mut history = rules.history(&activity, history_start, today, &charges);
        for benefit in &mut history {
            let logo = logos.get(&benefit.id).cloned().unwrap_or_default();
            benefit.logo_domain = logo.domain;
            benefit.logo_url = logo.url;
            benefit.logo_brand = logo.brand;
        }
        let others = own
            .iter()
            .filter(|transaction| transaction.other)
            .map(|transaction| transaction.id.as_str())
            .collect();
        CardBenefits {
            years: benefits::card_years(
                &history,
                &activity,
                &others,
                &settings.hidden_benefit_ids,
                today,
            ),
            use_first: benefits::use_first(&windows),
            account_id: card.id.clone(),
            product: rules.product.clone(),
            product_name: rules.name.clone(),
            fee: rules.fee_recovery(&own, transactions, &activity, &history, today),
            windows,
            history,
            review: rules.review(&own, &activity),
            activity,
        }
    };
    let cards = benefits::Catalog::from_settings(settings);
    let tracked = tracked_cards(accounts, settings, &cards);
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
    let plan = plan::plan(transactions, accounts, &expected, baseline, today);
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
    let cash = cashline::timeline(accounts, &horizon, &settings.planned_chapters, today);
    let year_ahead = live(forecast::forecast_within(
        transactions,
        &views,
        updated_at,
        today,
        subscriptions::HORIZON_DAYS,
    ));
    let subscriptions = subscriptions::subscriptions(&year_ahead, transactions);
    let credit = credit::credit(
        accounts,
        card.zip(statement.as_ref())
            .map(|(card, cycle)| (card.id.as_str(), cycle)),
        today,
    );
    let anomalies = anomalies::anomalies(transactions, baseline, today);
    let mut brief = attention::Brief {
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
    brief.needs.extend(attention::inferred(attention::Signals {
        today,
        transactions,
        new_series: &forecast::new_series(transactions, &year_ahead, today),
        pay: &forecast::pay_changes(transactions, &year_ahead, today),
        subscriptions: &subscriptions,
        anomalies: &anomalies,
    }));
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
        trips,
        horizon,
    }
}

/// Credit cards whose benefits are tracked, with their product's rules: the product the user
/// assigned, else the one the issuer's official name names, else, for the spending card only,
/// the catalog's only product.
fn tracked_cards<'a, 'c>(
    accounts: &'a [Account],
    settings: &FinanceSettings,
    cards: &'c benefits::Catalog,
) -> Vec<(&'a Account, &'c benefits::RuleSet)> {
    let card = spending_card(accounts, settings);
    accounts
        .iter()
        .filter(|account| account.r#type == "credit")
        .filter_map(|account| {
            let assigned = settings
                .benefit_cards
                .get(&account.id)
                .and_then(|product| cards.product(product))
                .or_else(|| cards.named_product(account.official_name.as_deref()));
            let spending = card.is_some_and(|card| card.id == account.id);
            let rules = if spending {
                assigned.or_else(|| cards.only())
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
    let cards = benefits::Catalog::from_settings(settings);
    for (card, rules) in tracked_cards(accounts, settings, &cards) {
        let own = transactions
            .iter()
            .filter(|transaction| transaction.account_id.as_deref() == Some(card.id.as_str()))
            .collect::<Vec<_>>();
        let activity = rules.for_card(&own).activity(&own, updated_at);
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
    saved_card(accounts, settings.spending_account_id.as_deref())
}

fn saved_card<'a>(accounts: &'a [Account], id: Option<&str>) -> Option<&'a Account> {
    id.and_then(|id| {
        accounts
            .iter()
            .find(|account| account.id == id && account.r#type == "credit")
    })
}

/// One spending period in full, from the view as it was derived: statements belong to the
/// saved card once its benefits say it is that card, as in `derive`.
pub fn spending_period(
    view: &View,
    request: &spending::PeriodRequest,
    zone: &dyn Zone,
) -> Option<spending::PeriodDetail> {
    let (snapshot, insights) = (&view.snapshot, view.insights.as_ref()?);
    let today = calendar::calendar_date(&snapshot.updated_at, zone)?;
    let card = saved_card(&snapshot.accounts, view.spending_account_id.as_deref());
    let statement = insights.statement.as_ref().filter(|_| {
        insights
            .benefits
            .as_ref()
            .zip(card)
            .is_some_and(|(benefits, card)| benefits.account_id == card.id)
    });
    spending::scope_list(
        &snapshot.transactions,
        &snapshot.accounts,
        card,
        statement,
        today,
    )
    .into_iter()
    .find(|scope| scope.id == request.scope && scope.owner == request.owner)?
    .detail(request.kind, request.offset)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod seed_fixture;

#[cfg(test)]
mod evals;
