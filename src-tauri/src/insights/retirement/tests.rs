use serde_json::json;

use super::*;
use crate::finance_contract::ContributionYear;
use crate::settings::RetirementProfile;
use crate::transaction_policy::{Classification, Kind, Mark};

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn account(id: &str, name: &str, kind: &str, treatment: &str, value: Option<f64>) -> Account {
    Account {
        id: id.into(),
        name: name.into(),
        r#type: kind.into(),
        tax_treatment: Some(treatment.into()),
        value,
        ..Account::default()
    }
}

fn row(
    id: &str,
    account: &str,
    date: &str,
    amount: f64,
    kind: Kind,
    flow: Option<&str>,
) -> Transaction {
    let mut value = json!({
        "id": id, "merchant": "Example", "category": "Transfer", "date": date,
        "amount": amount, "account": "Example", "accountId": account, "pending": false,
    });
    if let Some(flow) = flow {
        value["flow"] = json!(flow);
    }
    value["classification"] = serde_json::to_value(Classification {
        mark: Mark::Initial,
        spending: kind == Kind::Expense,
        kind,
        credit: false,
        zelle: false,
        others_spending: false,
    })
    .unwrap();
    serde_json::from_value(value).unwrap()
}

fn deferral(id: &str, account: &str, date: &str, amount: f64) -> Transaction {
    row(id, account, date, amount, Kind::Transfer, Some("deferral"))
}

fn ira_in(id: &str, date: &str, amount: f64, flow: &str) -> Transaction {
    row(id, "roth", date, amount, Kind::Transfer, Some(flow))
}

fn accounts() -> Vec<Account> {
    vec![
        account(
            "manual:401k",
            "401k",
            "retirement",
            "taxDeferred",
            Some(10_000.0),
        ),
        account("roth", "E*Trade", "retirement", "taxFree", Some(5_000.0)).with_subtype("ROTHIRA"),
        account(
            "brokerage",
            "Individual",
            "brokerage",
            "taxable",
            Some(2_500.0),
        ),
    ]
}

trait Subtype {
    fn with_subtype(self, subtype: &str) -> Self;
}

impl Subtype for Account {
    fn with_subtype(mut self, subtype: &str) -> Self {
        self.subtype = Some(subtype.into());
        self
    }
}

fn run(
    transactions: &[Transaction],
    accounts: &[Account],
    profile: RetirementProfile,
    today: &str,
) -> Retirement {
    run_stated(transactions, accounts, profile, today, &[])
}

fn run_stated(
    transactions: &[Transaction],
    accounts: &[Account],
    profile: RetirementProfile,
    today: &str,
    stated: &[ContributionYear],
) -> Retirement {
    let settings = FinanceSettings {
        retirement: Some(profile),
        ..FinanceSettings::default()
    };
    let recap = recap::recap(transactions, day(today));
    retirement(
        Inputs {
            transactions,
            accounts,
            settings: &settings,
            recap: &recap,
            stated,
            documents: &[],
        },
        day(today),
    )
}

#[test]
fn a_brokerages_own_note_names_the_tax_year_of_an_early_contribution() {
    let transactions = vec![
        ira_in("early", "2026-02-10", 2_000.0, "contribution"),
        ira_in("other", "2026-02-11", 500.0, "deposit"),
    ];
    let stated = [ContributionYear {
        account_id: "roth".into(),
        date: "2026-02-10".into(),
        tax_year: 2025,
        amount: 2_000.0,
    }];
    let result = run_stated(
        &transactions,
        &accounts(),
        RetirementProfile::default(),
        "2026-03-01",
        &stated,
    );
    let early = &result.deposits[0];
    assert_eq!(
        (early.counts_for, early.stated, early.waiting),
        (Some(2025), true, false)
    );
    // A note for another amount places nothing.
    assert!(result.deposits[1].waiting);
    assert_eq!(result.ira[0].contributed, 2_000.0);
}

#[test]
fn the_ira_deadline_moves_past_weekends_and_emancipation_day() {
    for (year, deadline) in [
        (2021, "2022-04-18"),
        (2022, "2023-04-18"),
        (2023, "2024-04-15"),
        (2026, "2027-04-15"),
    ] {
        assert_eq!(ira_deadline(year), day(deadline), "{year}");
    }
}

#[test]
fn names_decide_which_limit_an_account_counts_against() {
    let retirement = |name: &str, subtype: Option<&str>| Account {
        subtype: subtype.map(str::to_owned),
        ..account("id", name, "retirement", "taxDeferred", None)
    };
    assert_eq!(plan(&retirement("401k", None), None), Some(Plan::Workplace));
    assert_eq!(
        plan(&retirement("Savings", Some("403B")), None),
        Some(Plan::Workplace)
    );
    assert_eq!(
        plan(&retirement("E*Trade", Some("ROTHIRA")), None),
        Some(Plan::Ira)
    );
    assert_eq!(
        plan(&retirement("Old plan", None), Some("Rollover IRA")),
        Some(Plan::Ira)
    );
    assert_eq!(plan(&retirement("SEP IRA", None), None), None);
    assert_eq!(plan(&retirement("457(b) plan", None), None), None);
    assert_eq!(plan(&retirement("Mirae", None), None), None);
    let brokerage = account("id", "IRA lookalike", "brokerage", "taxable", None);
    assert_eq!(plan(&brokerage, None), None);
}

#[test]
fn deferrals_across_plans_count_once_per_paycheck_and_employer_money_does_not() {
    let mut accounts = accounts();
    accounts.push(account(
        "manual:403b",
        "Old 403(b)",
        "retirement",
        "taxDeferred",
        Some(1.0),
    ));
    let transactions = vec![
        // One paycheck buys two funds.
        deferral("a1", "manual:401k", "2026-09-04", 300.0),
        deferral("a2", "manual:401k", "2026-09-04", 200.0),
        deferral("b", "manual:401k", "2026-09-18", 500.0),
        deferral("c", "manual:401k", "2026-10-02", 500.0),
        row(
            "match",
            "manual:401k",
            "2026-10-02",
            250.0,
            Kind::Transfer,
            Some("employer"),
        ),
        // An earlier job's plan shares the limit.
        deferral("old", "manual:403b", "2026-02-06", 1_000.0),
        deferral("last-year", "manual:401k", "2025-12-19", 900.0),
    ];
    let result = run(
        &transactions,
        &accounts,
        RetirementProfile::default(),
        "2026-10-15",
    );
    let workplace = result.workplace.clone().unwrap();
    assert_eq!(workplace.limit, Some(24_500.0));
    assert_eq!(workplace.deferred, 2_500.0);
    assert_eq!(workplace.employer, 250.0);
    assert_eq!(workplace.through, Some(day("2026-10-02")));
    // Every 14 days after Oct 2: Oct 16 through Dec 25.
    assert_eq!(workplace.paychecks_left, Some(6));
    assert_eq!(workplace.projected, Some(5_500.0));
    assert_eq!(workplace.per_paycheck_to_max, Some(3_666.66));
    assert!(needs(&result).is_empty());
}

#[test]
fn catch_up_follows_age_at_the_end_of_the_year() {
    let limit = |born| {
        let profile = RetirementProfile {
            birth_year: Some(born),
            ..RetirementProfile::default()
        };
        let result = run(&[], &accounts(), profile, "2026-06-01");
        (result.workplace.unwrap().limit, result.ira[0].limit)
    };
    assert_eq!(limit(2004), (Some(24_500.0), Some(7_500.0)));
    assert_eq!(limit(1976), (Some(32_500.0), Some(8_600.0)));
    assert_eq!(limit(1964), (Some(35_750.0), Some(8_600.0)));
}

#[test]
fn a_year_without_published_limits_has_none() {
    let result = run(&[], &accounts(), RetirementProfile::default(), "2030-06-01");
    assert_eq!(result.workplace.unwrap().limit, None);
    assert_eq!(result.ira[0].limit, None);
}

#[test]
fn ira_inflows_count_by_tax_year_and_ask_when_they_could_be_either() {
    let transactions = vec![
        ira_in("early", "2026-02-10", 2_000.0, "contribution"),
        ira_in("may", "2026-05-12", 1_000.0, "contribution"),
        ira_in("moved", "2026-06-01", 4_000.0, "transfer"),
        ira_in("chosen", "2026-03-01", 500.0, "deposit"),
        ira_in("rollover", "2026-03-02", 9_000.0, "deposit"),
    ];
    let profile = RetirementProfile {
        contribution_years: [("chosen".into(), Some(2025)), ("rollover".into(), None)].into(),
        ..RetirementProfile::default()
    };
    let years = |result: &Retirement| {
        result
            .ira
            .iter()
            .map(|year| (year.tax_year, year.contributed))
            .collect::<Vec<_>>()
    };
    let result = run(&transactions, &accounts(), profile.clone(), "2026-06-15");
    assert_eq!(years(&result), vec![(2026, 1_000.0)]);
    // Every deposit this year is listed with where it stands, so a choice can be changed.
    let placed = result
        .deposits
        .iter()
        .map(|deposit| {
            (
                deposit.transaction_id.as_str(),
                deposit.years.clone(),
                deposit.counts_for,
                deposit.chosen,
                deposit.waiting,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        placed,
        vec![
            ("early", vec![2025, 2026], None, false, true),
            ("chosen", vec![2025, 2026], Some(2025), true, false),
            ("rollover", vec![2025, 2026], None, true, false),
            ("may", vec![2026], Some(2026), false, false),
            ("moved", vec![2026], None, false, true),
        ]
    );
    let need = &needs(&result)[0];
    assert!(matches!(
        need.item,
        Item::ContributionsToConfirm { count: 2, amount, .. } if amount == 6_000.0
    ));
    assert_eq!(need.id, "contributions-to-confirm:moved");

    // Until 2025's deadline passes, both years show.
    let earlier = run(&transactions, &accounts(), profile, "2026-04-10");
    assert_eq!(years(&earlier), vec![(2025, 500.0), (2026, 0.0)]);
}

#[test]
fn roth_basis_adds_counted_contributions_and_withdrawals_come_out_of_it_first() {
    let transactions = vec![
        ira_in("first", "2025-06-01", 3_000.0, "contribution"),
        ira_in("second", "2026-06-01", 2_000.0, "contribution"),
        ira_in("rollover", "2026-07-01", 9_000.0, "transfer"),
        row(
            "out",
            "roth",
            "2026-08-01",
            -1_500.0,
            Kind::Transfer,
            Some("withdrawal"),
        ),
    ];
    let profile = RetirementProfile {
        earlier_contributions: [("roth".into(), 1_000.0)].into(),
        ..RetirementProfile::default()
    };
    let result = run(&transactions, &accounts(), profile, "2026-09-01");
    let basis = &result.roth_basis[0];
    assert_eq!(basis.since, Some(day("2025-06-01")));
    assert_eq!(basis.earlier, Some(1_000.0));
    assert_eq!(basis.contributed, 5_000.0);
    assert_eq!(basis.withdrawn, 1_500.0);
    assert_eq!(basis.basis, 4_500.0);
}

#[test]
fn an_excess_ira_contribution_is_urgent() {
    let transactions = vec![
        ira_in("a", "2026-05-01", 5_000.0, "contribution"),
        ira_in("b", "2026-09-01", 3_000.0, "contribution"),
    ];
    let result = run(
        &transactions,
        &accounts(),
        RetirementProfile::default(),
        "2026-10-15",
    );
    let need = &needs(&result)[0];
    assert_eq!(need.severity, Severity::Urgent);
    assert!(matches!(
        need.item,
        Item::ContributionLimit { plan: "ira", tax_year: 2026, on_pace: false, amount, .. }
            if amount == 8_000.0
    ));
}

#[test]
fn deferrals_on_pace_past_the_limit_raise_a_need_before_they_pass_it() {
    let transactions = (0..3)
        .map(|index| {
            let date = day("2026-09-04") + Duration::days(14 * index);
            deferral(
                &format!("d{index}"),
                "manual:401k",
                &date.to_string(),
                5_000.0,
            )
        })
        .collect::<Vec<_>>();
    let result = run(
        &transactions,
        &accounts(),
        RetirementProfile::default(),
        "2026-10-15",
    );
    let need = &needs(&result)[0];
    assert_eq!(need.severity, Severity::Attention);
    assert!(matches!(
        need.item,
        Item::ContributionLimit { plan: "workplace", on_pace: true, amount, .. }
            if amount == 45_000.0
    ));
}

#[test]
fn savings_add_deferrals_to_take_home_pay_and_leave_investment_income_out() {
    let transactions = vec![
        row(
            "pay-2025",
            "checking",
            "2025-01-03",
            3_000.0,
            Kind::Income,
            None,
        ),
        row("pay", "checking", "2026-03-06", 4_000.0, Kind::Income, None),
        row(
            "rent",
            "checking",
            "2026-03-07",
            -1_000.0,
            Kind::Expense,
            None,
        ),
        row(
            "dividend",
            "brokerage",
            "2026-03-08",
            900.0,
            Kind::Dividend,
            None,
        ),
        deferral("d", "manual:401k", "2026-03-06", 1_000.0),
    ];
    let result = run(
        &transactions,
        &accounts(),
        RetirementProfile::default(),
        "2026-10-15",
    );
    let this_year = &result.savings[0];
    assert_eq!(
        (
            this_year.year,
            this_year.take_home,
            this_year.spending,
            this_year.deferred
        ),
        (2026, 4_000.0, 1_000.0, 1_000.0)
    );
    assert!(!this_year.partial);
    // (4,000 + 1,000 − 1,000) ÷ 5,000.
    assert_eq!(this_year.rate, Some(80.0));
    // History starts January 3, so 2025 isn't whole and isn't shown.
    assert_eq!(result.savings.len(), 1);
}

#[test]
fn coverage_needs_a_full_year_of_spending_and_adds_planned_costs() {
    let accounts = accounts();
    let short = vec![row(
        "rent",
        "checking",
        "2026-03-07",
        -1_000.0,
        Kind::Expense,
        None,
    )];
    assert!(run(
        &short,
        &accounts,
        RetirementProfile::default(),
        "2026-10-15"
    )
    .coverage
    .is_none());

    let year = vec![
        row("old", "checking", "2025-01-02", -10.0, Kind::Expense, None),
        row(
            "rent",
            "checking",
            "2026-03-07",
            -6_000.0,
            Kind::Expense,
            None,
        ),
    ];
    let profile = RetirementProfile {
        planned_monthly: Some(500.0),
        ..RetirementProfile::default()
    };
    let coverage = run(&year, &accounts, profile, "2026-10-15")
        .coverage
        .unwrap();
    assert_eq!(coverage.invested, 17_500.0);
    assert!(coverage.complete);
    assert_eq!((coverage.spending, coverage.planned), (6_000.0, 6_000.0));
    assert_eq!(coverage.years, 1.46);
}

#[test]
fn buckets_follow_tax_treatment_and_say_when_a_value_is_missing() {
    let mut accounts = accounts();
    accounts.push(account("hsa", "HSA", "retirement", "taxFree", None));
    let result = run(&[], &accounts, RetirementProfile::default(), "2026-10-15");
    assert_eq!(
        (
            result.buckets.pre_tax,
            result.buckets.tax_free,
            result.buckets.taxable
        ),
        (10_000.0, 5_000.0, 2_500.0)
    );
    assert!(!result.buckets.complete);
    assert_eq!(result.uncounted, vec!["hsa"]);
}
