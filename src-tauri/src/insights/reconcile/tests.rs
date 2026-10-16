use super::*;

fn performance(points: &[(&str, f64, f64, f64)]) -> Vec<Performance> {
    let points: Vec<_> = points
        .iter()
        .map(|(date, value, deposits, benchmark)| {
            serde_json::json!({
                "date": date, "value": value, "netDeposits": deposits, "sp500": benchmark,
            })
        })
        .collect();
    serde_json::from_value(serde_json::json!([
        { "accountId": "total", "name": "Combined", "institution": "2 accounts",
          "currentValue": 0.0, "performanceMethod": "time-weighted", "points": points },
        { "accountId": "broker", "name": "Broker", "institution": "Synthetic",
          "currentValue": 0.0, "points": [] },
    ]))
    .unwrap()
}

fn trade(
    kind: &str,
    date: &str,
    ticker: &str,
    units: f64,
    amount: f64,
    gain: Option<f64>,
) -> Trade {
    serde_json::from_value(serde_json::json!({
        "id": format!("{kind}-{date}-{ticker}"), "type": kind, "date": date, "amount": amount,
        "account": "Broker", "accountId": "broker", "ticker": ticker, "units": units,
        "estimatedRealizedGain": gain,
    }))
    .unwrap()
}

fn cash(kind: &str, date: &str, amount: f64) -> Transaction {
    serde_json::from_value(serde_json::json!({
        "id": format!("{kind}-{date}-{amount}"), "merchant": kind, "category": kind,
        "date": date, "amount": amount, "account": "Broker", "accountId": "broker",
        "pending": false, "description": null,
        "classification": {
            "mark": if kind == "tax" { "cash" } else { kind },
            "kind": kind, "spending": false, "zelle": false,
        },
    }))
    .unwrap()
}

fn holding(ticker: &str, shares: f64, value: f64) -> Holding {
    serde_json::from_value(serde_json::json!({
        "ticker": ticker, "name": ticker, "accountId": "broker", "shares": shares,
        "price": value / shares, "value": value, "costBasis": null,
        "dailyChangePct": null, "totalChangePct": null, "color": "#000000",
    }))
    .unwrap()
}

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

/// A synthetic month: 10,000 grows, 2,000 arrives on the 5th, 500 leaves on the 9th.
fn month() -> Vec<Performance> {
    performance(&[
        ("2026-09-01", 10_000.00, 10_000.00, 10_000.00),
        ("2026-09-02", 10_150.00, 10_000.00, 10_080.00),
        ("2026-09-03", 10_090.00, 10_000.00, 10_050.00),
        ("2026-09-04", 10_300.00, 10_000.00, 10_150.00),
        ("2026-09-08", 12_420.00, 12_000.00, 12_200.00),
        ("2026-09-09", 12_010.00, 11_500.00, 11_760.00),
        ("2026-09-10", 12_133.37, 11_500.00, 11_800.00),
    ])
}

#[test]
fn the_waterfall_ties_to_the_cent_and_the_identity_holds() {
    let trades = vec![
        trade("SELL", "2026-09-04", "AAA", 2.0, 210.0, Some(37.25)),
        trade("BUY", "2026-09-08", "BBB", 3.0, -300.0, None),
        // Reported only as a reinvestment, a dividend still counts once.
        trade("REINVESTMENT", "2026-09-09", "BBB", 0.1, -11.11, None),
    ];
    let transactions = vec![
        cash("dividend", "2026-09-03", 18.40),
        cash("interest", "2026-09-04", 2.07),
        cash("fee", "2026-09-08", -4.99),
        cash("tax", "2026-09-09", -1.50),
        // Outside the period, or another account's, it is not booked.
        cash("dividend", "2026-08-30", 99.0),
    ];
    let statement = statement(&month(), &trades, &transactions, day("2026-09-01")).unwrap();
    // Start + deposits − withdrawals + gain = end.
    assert_eq!(statement.start_value, 10_000.00);
    assert_eq!(statement.deposits, 2_000.00);
    assert_eq!(statement.withdrawals, 500.00);
    assert_eq!(statement.net_flows, 1_500.00);
    assert_eq!(statement.investment_gain, 633.37);
    let w = &statement.waterfall;
    let cents = |value: f64| minor_units(value);
    assert_eq!(
        cents(w.start) + cents(w.deposits) - cents(w.withdrawals)
            + cents(w.price_gain)
            + cents(w.dividends)
            + cents(w.interest)
            - cents(w.fees)
            - cents(w.taxes),
        cents(w.end)
    );
    assert_eq!(w.end, 12_133.37);
    assert_eq!(w.dividends, 29.51);
    assert_eq!((w.interest, w.fees, w.taxes), (2.07, 4.99, 1.50));
    assert_eq!(w.realized, Some(37.25));
    assert_eq!(
        cents(w.realized.unwrap()) + cents(w.unrealized.unwrap()),
        cents(w.price_gain)
    );
    // A sale without a known cost leaves the split unknown, never guessed; the total stays.
    let unknown = vec![trade("SELL", "2026-09-04", "AAA", 2.0, 210.0, None)];
    let partial = super::statement(&month(), &unknown, &transactions, day("2026-09-01"))
        .unwrap()
        .waterfall;
    assert_eq!((partial.realized, partial.unrealized), (None, None));
    // Without the reinvestment, only the cash dividend is booked.
    assert_eq!(partial.price_gain, 619.39);
}

#[test]
fn two_labeled_returns_beside_the_benchmark_and_nothing_annualized_under_a_year() {
    let statement = statement(&month(), &[], &[], day("2026-09-01")).unwrap();
    // Each day linked, the deposit and withdrawal left out of the change.
    let linked = (10_150.0 / 10_000.0)
        * (10_090.0 / 10_150.0)
        * (10_300.0 / 10_090.0)
        * ((12_420.0 - 2_000.0) / 10_300.0)
        * ((12_010.0 + 500.0) / 12_420.0)
        * (12_133.37 / 12_010.0);
    assert_eq!(statement.time_weighted_pct, round2((linked - 1.0) * 100.0));
    assert!(statement.money_weighted_pct.is_some());
    assert_eq!(statement.annualized, None);
    let benchmark = statement.benchmark_pct.unwrap();
    assert_eq!(
        statement.active_pct,
        Some(round2(statement.time_weighted_pct - benchmark))
    );
    // The drawdown is measured on the linked index, so the withdrawal isn't a fall.
    let drawdown = statement.risk.drawdown_pct.unwrap();
    assert!(drawdown < 0.0 && drawdown > -1.0, "{drawdown}");
    // Too few sessions to say anything about volatility or beta.
    assert_eq!(statement.risk.volatility_pct, None);
    assert_eq!(statement.risk.beta, None);
}

#[test]
fn a_year_or_more_is_annualized_and_risk_needs_enough_sessions() {
    let mut points = Vec::new();
    let mut value = 1_000.0;
    let mut benchmark = 1_000.0;
    let mut date = day("2025-01-01");
    let mut session = 0;
    while date <= day("2026-01-10") {
        if is_trading_day(date) {
            let market = if session % 2 == 0 { 0.01 } else { -0.008 };
            value *= 1.0 + 1.5 * market;
            benchmark *= 1.0 + market;
            session += 1;
        }
        points.push((date.to_string(), value, 1_000.0, benchmark));
        date = date.succ_opt().unwrap();
    }
    let points: Vec<(&str, f64, f64, f64)> = points
        .iter()
        .map(|(date, value, deposits, benchmark)| (date.as_str(), *value, *deposits, *benchmark))
        .collect();
    let statement = statement(&performance(&points), &[], &[], day("2025-01-01")).unwrap();
    assert!(statement.annualized.is_some());
    let beta = statement.risk.beta.unwrap();
    assert!((beta.beta - 1.5).abs() < 0.01, "{beta:?}");
    assert!(beta.r_squared > 0.99);
    assert!(statement.risk.volatility_pct.unwrap() > 10.0);
}

#[test]
fn statements_cover_each_window_the_history_reaches() {
    let windows: Vec<Option<Window>> = statements(&month(), &[], &[])
        .into_iter()
        .map(|statement| statement.window)
        .collect();
    assert_eq!(windows, [Some(Window::Week), Some(Window::All)]);
    // Without flows there is no statement.
    let mut value_only = month();
    value_only[0].performance_method = Some("value-only".into());
    assert!(statements(&value_only, &[], &[]).is_empty());
}

/// Two holdings and cash over three days, with closes adjusted for dividends.
fn priced() -> (Vec<Performance>, Vec<Holding>, DailyCloses) {
    // AAA: 10 shares at 100, then 105, 103. BBB: 20 shares at 50, then 49, 51 (bought 5 on
    // the 3rd). Cash 500 throughout, then 250 after the buy.
    let performance = performance(&[
        ("2026-09-01", 1_000.0 + 1_000.0 + 500.0, 2_500.0, 2_500.0),
        ("2026-09-02", 1_050.0 + 980.0 + 500.0, 2_500.0, 2_520.0),
        ("2026-09-03", 1_030.0 + 1_275.0 + 245.0, 2_500.0, 2_510.0),
    ]);
    let holdings = vec![holding("AAA", 10.0, 1_030.0), holding("BBB", 25.0, 1_275.0)];
    let closes = BTreeMap::from([
        (
            "AAA".to_string(),
            vec![
                (day("2026-09-01"), 100.0),
                (day("2026-09-02"), 105.0),
                (day("2026-09-03"), 103.0),
            ],
        ),
        (
            "BBB".to_string(),
            vec![
                (day("2026-09-01"), 50.0),
                (day("2026-09-02"), 49.0),
                (day("2026-09-03"), 51.0),
            ],
        ),
    ]);
    (performance, holdings, closes)
}

#[test]
fn carino_contributions_sum_to_the_period_return_and_dollars_to_the_gain() {
    let (performance, holdings, closes) = priced();
    let trades = vec![trade("BUY", "2026-09-03", "BBB", 5.0, -255.0, None)];
    let report = attribute(
        &performance,
        &holdings,
        &trades,
        &closes,
        day("2026-09-01"),
        None,
    )
    .unwrap();
    let statement = statement(&performance, &trades, &[], day("2026-09-01")).unwrap();
    // The parts sum to the period's return and its gain exactly as shown.
    let points: f64 = report
        .contributions
        .iter()
        .map(|item| item.contribution_pct)
        .sum();
    assert!(
        (points - statement.time_weighted_pct).abs() < 1e-9,
        "{points}"
    );
    let dollars: i64 = report
        .contributions
        .iter()
        .map(|item| minor_units(item.dollars))
        .sum();
    assert_eq!(dollars, minor_units(statement.investment_gain));
    // And before rounding, the smoothed parts and the cash day-parts sum to the return.
    let portfolio = [2_530.0 / 2_500.0 - 1.0, 2_550.0 / 2_530.0 - 1.0];
    let parts = vec![
        vec![1_000.0 / 2_500.0 * 0.05, 1_000.0 / 2_500.0 * -0.02],
        vec![
            1_050.0 / 2_530.0 * (103.0 / 105.0 - 1.0),
            980.0 / 2_530.0 * (51.0 / 49.0 - 1.0),
        ],
    ];
    let smoothed = carino(&portfolio, &parts);
    let cash: Vec<Vec<f64>> = portfolio
        .iter()
        .zip(&parts)
        .map(|(r, day)| vec![r - day.iter().sum::<f64>()])
        .collect();
    let period = portfolio.iter().fold(1.0, |g, r| g * (1.0 + r)) - 1.0;
    let total = smoothed.iter().sum::<f64>() + carino(&portfolio, &cash)[0];
    assert!((total - period).abs() < 1e-12, "{total} {period}");
    // Drivers in dollars, movers by their own return; a traded holding is still a subject.
    let aaa = report
        .contributions
        .iter()
        .find(|item| item.key == "AAA")
        .unwrap();
    assert_eq!(aaa.dollars, 30.0);
    assert_eq!(aaa.return_pct, Some(3.0));
    let bbb = report
        .contributions
        .iter()
        .find(|item| item.key == "BBB")
        .unwrap();
    assert!(bbb.traded);
    // 20 shares lose 20, then 20 shares gain 40 (the 5 bought on the 3rd don't count yet).
    assert_eq!(
        bbb.dollars,
        round2(-20.0 + 20.0 * 49.0 * (51.0 / 49.0 - 1.0))
    );
    assert_eq!(bbb.buy_and_hold, Some(20.0));
    // Its weight moved with prices and with the shares bought; together, the whole change.
    let (drift, traded) = (bbb.drift_pct.unwrap(), bbb.traded_pct.unwrap());
    assert!((drift + traded - (bbb.end_weight_pct - bbb.start_weight_pct)).abs() < 0.011);
    assert!(traded > 0.0);
    assert_eq!(report.gains, ["AAA", "BBB"]);
    assert!(report.losses.is_empty());
    assert_eq!(report.risers, ["AAA", "BBB"]);
    assert_eq!(report.buy_and_hold, Some(50.0));
    assert_eq!(report.trading, Some(0.0));
    assert!(report.top_three_share_pct.is_some());
    // One holding's page names only it; the rest join the cash row.
    let one = attribute(
        &performance,
        &holdings,
        &trades,
        &closes,
        day("2026-09-01"),
        Some("AAA"),
    )
    .unwrap();
    assert_eq!(one.contributions.len(), 2);
}

#[test]
fn a_holding_whose_trades_dont_explain_its_shares_is_unpriced_not_guessed() {
    let (performance, holdings, mut closes) = priced();
    // A sale with no share count can't rebuild the history.
    let trades = vec![trade("SELL", "2026-09-02", "AAA", f64::NAN, 10.0, None)];
    closes.remove("BBB");
    let report = attribute(
        &performance,
        &holdings,
        &trades,
        &closes,
        day("2026-09-01"),
        None,
    )
    .unwrap();
    assert_eq!(report.unpriced, ["AAA", "BBB"]);
    assert_eq!(report.contributions.len(), 1);
    assert_eq!(report.contributions[0].key, CASH);
}

#[test]
fn exposure_counts_effective_holdings_and_the_top_five() {
    let ten: Vec<Holding> = (0..9)
        .map(|index| holding(&format!("T{index}"), 1.0, 600.0 / 9.0))
        .chain([holding("BIG", 1.0, 400.0)])
        .collect();
    let exposure = exposure(&month(), &ten).unwrap();
    assert_eq!(exposure.holdings, 10);
    // One holding at 40% beside nine equal ones makes five.
    assert_eq!(exposure.effective_holdings, 5.0);
    assert_eq!(exposure.top_five_pct, 66.7);
}
