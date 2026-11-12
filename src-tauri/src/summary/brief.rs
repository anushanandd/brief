//! The day in a few lines: the move and the market's, the holdings that led it, and up to two
//! headlines quoted as published (`headlines`). Every figure is Rust's; every reason is a
//! publisher's, credited and linked. A quiet day is one line.
use super::headlines::Headline;
use super::*;

/// A day the portfolio moved less than this is quiet: its figures alone.
const QUIET_PCT: f64 = 0.25;
/// Holdings named as leading the day, at most, each at least this share of the day's dollars.
const LEADERS: usize = 3;
const LEADER_SHARE: f64 = 0.15;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    Up,
    Down,
    Flat,
}

/// One piece of a line. React styles each and adds no words of its own.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Span {
    Text {
        text: String,
    },
    /// A figure Rust computed and formatted. A personal amount is `sensitive`, so screen
    /// privacy can mask it.
    Figure {
        text: String,
        tone: Tone,
        sensitive: bool,
    },
    Subject {
        text: String,
        ticker: Option<String>,
    },
}

fn text(value: impl Into<String>) -> Span {
    Span::Text { text: value.into() }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn tone(value: f64) -> Tone {
    match round1(value) {
        rounded if rounded > 0.0 => Tone::Up,
        rounded if rounded < 0.0 => Tone::Down,
        _ => Tone::Flat,
    }
}

fn grouped(whole: u64) -> String {
    let digits = whole.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn sign(value: f64) -> &'static str {
    if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "\u{2212}"
    } else {
        ""
    }
}

/// "0.9%", unsigned: the verb carries the way.
fn pct(value: f64) -> Span {
    Span::Figure {
        text: format!("{:.1}%", round1(value).abs()),
        tone: tone(value),
        sensitive: false,
    }
}

/// "+6.1%", signed with a true minus.
fn signed_pct(value: f64) -> Span {
    let rounded = round1(value);
    Span::Figure {
        text: format!("{}{:.1}%", sign(rounded), rounded.abs()),
        tone: tone(value),
        sensitive: false,
    }
}

/// "$2.95" or "−$0.18": earnings per share, a public figure.
fn eps(value: f64) -> String {
    let cents = (value * 100.0).round() / 100.0;
    format!(
        "{}${:.2}",
        if cents < 0.0 { "\u{2212}" } else { "" },
        cents.abs()
    )
}

/// An earnings report in a line: what the company earned per share against the consensus,
/// then how the stock's own move answered, when it was more than ordinary.
pub(super) fn earnings_line(
    ticker: &str,
    reported: Option<f64>,
    estimate: Option<f64>,
    reaction: Option<(f64, f64)>,
) -> Vec<Span> {
    let mut line = vec![subject(ticker)];
    match (reported, estimate) {
        (Some(reported), Some(estimate)) => {
            let (reported_text, estimate_text) = (eps(reported), eps(estimate));
            line.push(text(" reported "));
            line.push(Span::Figure {
                text: reported_text.clone(),
                tone: Tone::Flat,
                sensitive: false,
            });
            line.push(text(" a share, "));
            line.push(text(if reported_text == estimate_text {
                "matching the "
            } else if reported > estimate {
                "above the "
            } else {
                "below the "
            }));
            line.push(Span::Figure {
                text: estimate_text,
                tone: Tone::Flat,
                sensitive: false,
            });
            line.push(text(" analysts expected."));
        }
        (Some(reported), None) => {
            line.push(text(" reported "));
            line.push(Span::Figure {
                text: eps(reported),
                tone: Tone::Flat,
                sensitive: false,
            });
            line.push(text(" a share."));
        }
        _ => line.push(text(" reported its quarter.")),
    }
    if let Some((own, typical)) = reaction.filter(|(_, typical)| *typical > 0.0) {
        let times = own.abs() / typical;
        if times >= 1.0 {
            line.push(text(" Its own move over the next two sessions was "));
            line.push(signed_pct(own));
            line.push(text(format!(", {times:.1}× a typical two days.")));
        } else {
            line.push(text(" Its own move after was ordinary."));
        }
    }
    line
}

/// "−$1,240", whole dollars with a true minus; a personal amount.
fn signed_money(value: f64) -> Span {
    let whole = value.round();
    Span::Figure {
        text: format!("{}${}", sign(whole), grouped(whole.abs() as u64)),
        tone: tone(value),
        sensitive: true,
    }
}

fn verb(value: f64) -> &'static str {
    match tone(value) {
        Tone::Up => "rose",
        Tone::Down => "fell",
        Tone::Flat => "was flat",
    }
}

fn subject(ticker: &str) -> Span {
    Span::Subject {
        text: ticker.into(),
        ticker: Some(ticker.into()),
    }
}

/// "today", or the session's day when it isn't today's (before the open, or a weekend).
fn when(session: &SessionClock) -> String {
    if session.date == session.today {
        "today".into()
    } else if (session.today - session.date).num_days() < 7 {
        format!("on {}", session.date.format("%A"))
    } else {
        format!("on {}", session.date.format("%b %-d"))
    }
}

/// "A, B and C".
fn joined(spans: Vec<Vec<Span>>) -> Vec<Span> {
    let count = spans.len();
    let mut out = Vec::new();
    for (index, part) in spans.into_iter().enumerate() {
        if index > 0 {
            out.push(text(if index == count - 1 { " and " } else { ", " }));
        }
        out.extend(part);
    }
    out
}

/// The day in three parts: the market's wrap, your move with the reasons for the holdings that
/// led it, and what changed.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "DayBrief")]
pub struct Brief {
    /// The market's wrap as published, with its summary, when the S&P 500 moved enough to
    /// have one.
    pub market: Option<Headline>,
    pub lines: Vec<Vec<Span>>,
    /// A reason for each holding that led the day by its own move, quoted from a story.
    pub headlines: Vec<Headline>,
    /// The period's events that changed something: filings, corporate actions, and company
    /// news not already quoted, most consequential first.
    pub changed: Vec<super::ListedEvent>,
    /// Earnings reported in the latest two sessions, newest first.
    pub earnings: Vec<super::EarningsDigest>,
}

/// The holdings that led the day: moving its way, largest dollars first, each a real share.
pub(super) fn leaders(today: &TodayMove) -> Vec<&TodayPosition> {
    let Some(total) = today.change.filter(|total| *total != 0.0) else {
        return Vec::new();
    };
    let mut leading: Vec<&TodayPosition> = today
        .positions
        .iter()
        .filter(|position| position.change.signum() == total.signum())
        .filter(|position| position.change.abs() >= LEADER_SHARE * total.abs())
        .collect();
    leading.sort_by(|a, b| b.change.abs().total_cmp(&a.change.abs()));
    leading.truncate(LEADERS);
    leading
}

/// Whether the day was too small to say more than its figures.
pub(super) fn quiet(today: &TodayMove) -> bool {
    today
        .change_pct
        .is_none_or(|change_pct| change_pct.abs() < QUIET_PCT)
}

/// The lines for the portfolio, or for one holding with the dollars its position moved.
pub(super) fn lines(
    today: &TodayMove,
    ticker: Option<&str>,
    position_change: Option<f64>,
    session: &SessionClock,
) -> Vec<Vec<Span>> {
    let Some(change_pct) = today.change_pct else {
        return Vec::new();
    };
    let mut first = vec![match ticker {
        Some(ticker) => subject(ticker),
        None => text("Your investments"),
    }];
    if tone(change_pct) == Tone::Flat {
        let be = if ticker.is_some() { "was" } else { "were" };
        first.push(text(format!(" {be} flat")));
        first.push(text(format!(" {}.", when(session))));
    } else {
        first.push(text(format!(" {} ", verb(change_pct))));
        first.push(pct(change_pct));
        if let Some(dollars) = today.change.or(position_change) {
            first.push(text(" ("));
            first.push(signed_money(dollars));
            first.push(text(")"));
        }
        first.push(text(format!(" {}.", when(session))));
    }
    if let Some(comparison) = today
        .market
        .as_ref()
        .map(|market| against(change_pct, market))
    {
        // "today." becomes "today, in line with the S&P 500."
        first.pop();
        first.push(text(format!(" {},", when(session))));
        first.extend(comparison);
        first.push(text("."));
    }
    let mut lines = vec![first];
    if ticker.is_none() && !quiet(today) {
        let leading = leaders(today);
        if !leading.is_empty() {
            // The table lists each holding's move; the summary only names who led.
            let mut second = joined(
                leading
                    .iter()
                    .map(|position| vec![subject(&position.ticker)])
                    .collect(),
            );
            second.push(text(if change_pct < 0.0 {
                " led the drop."
            } else {
                " led the gain."
            }));
            lines.push(second);
        }
    }
    lines
}

/// How the move compares with the S&P 500's: in line, more, less, or the other way.
fn against(change_pct: f64, market: &Benchmark) -> Vec<Span> {
    /// Moves this close read as the market's.
    const IN_LINE_PCT: f64 = 0.2;
    let market_pct = market.change_pct;
    match (tone(change_pct), tone(market_pct)) {
        (_, Tone::Flat) => vec![text(" while the S&P 500 was flat")],
        (mine, theirs) if mine != theirs => vec![
            text(format!(" while the S&P 500 {} ", verb(market_pct))),
            pct(market_pct),
        ],
        _ if (change_pct.abs() - market_pct.abs()).abs() < IN_LINE_PCT => {
            vec![text(" in line with the S&P 500")]
        }
        _ => vec![
            text(if change_pct.abs() > market_pct.abs() {
                " more than the S&P 500\u{2019}s "
            } else {
                " less than the S&P 500\u{2019}s "
            }),
            pct(market_pct),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(now: &str) -> SessionClock {
        SessionClock::at(now.parse().unwrap())
    }

    fn written(line: &[Span]) -> String {
        line.iter()
            .map(|span| match span {
                Span::Text { text } | Span::Figure { text, .. } | Span::Subject { text, .. } => {
                    text.as_str()
                }
            })
            .collect()
    }

    fn day(change_pct: f64, change: f64, positions: &[(&str, f64, f64)]) -> TodayMove {
        TodayMove {
            as_of: None,
            change_pct: Some(change_pct),
            change: Some(change),
            market: Some(Benchmark {
                symbol: "SPY".into(),
                name: "S&P 500".into(),
                change_pct: -0.7,
            }),
            sector: None,
            kind: None,
            positions: positions
                .iter()
                .map(|(ticker, change_pct, change)| TodayPosition {
                    ticker: (*ticker).into(),
                    change_pct: *change_pct,
                    change: *change,
                    kind: None,
                })
                .collect(),
            sectors: Vec::new(),
        }
    }

    #[test]
    fn the_day_reads_as_figures_then_the_holdings_that_led_it() {
        // Thursday, Oct 8, 2026, after the close, in New York.
        let session = clock("2026-10-08T21:00:00Z");
        let today = day(
            -0.9,
            -1_240.4,
            &[
                ("NVDA", -2.1, -610.0),
                ("TEST", -3.4, -410.0),
                ("VTI", -0.4, -120.0),
                ("BND", 0.2, 30.0),
            ],
        );
        let lines = lines(&today, None, None, &session);
        let written: Vec<String> = lines.iter().map(|line| written(line)).collect();
        assert_eq!(
            written,
            [
                "Your investments fell 0.9% (\u{2212}$1,240) today, more than the S&P 500\u{2019}s 0.7%.",
                "NVDA and TEST led the drop."
            ]
        );
        // Personal dollars are maskable; prices aren't.
        assert!(lines[0].contains(&Span::Figure {
            text: "\u{2212}$1,240".into(),
            tone: Tone::Down,
            sensitive: true
        }));
    }

    #[test]
    fn an_earnings_report_reads_against_the_consensus_and_the_stocks_own_answer() {
        assert_eq!(
            written(&earnings_line("TEST", Some(0.42), Some(0.38), Some((6.1, 2.6)))),
            "TEST reported $0.42 a share, above the $0.38 analysts expected. Its own move over the next two sessions was +6.1%, 2.3× a typical two days."
        );
        assert_eq!(
            written(&earnings_line("TEST", Some(-0.18), Some(-0.18), Some((0.4, 2.6)))),
            "TEST reported \u{2212}$0.18 a share, matching the \u{2212}$0.18 analysts expected. Its own move after was ordinary."
        );
        assert_eq!(
            written(&earnings_line("TEST", Some(1.0), None, None)),
            "TEST reported $1.00 a share."
        );
        assert_eq!(
            written(&earnings_line("TEST", None, Some(1.0), None)),
            "TEST reported its quarter."
        );
    }

    #[test]
    fn a_quiet_day_is_one_line_and_a_holding_names_itself() {
        let session = clock("2026-10-08T21:00:00Z");
        let quiet = day(0.1, 120.0, &[("NVDA", 0.4, 90.0)]);
        let lines = lines(&quiet, None, None, &session);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            written(&lines[0]),
            "Your investments rose 0.1% (+$120) today, while the S&P 500 fell 0.7%."
        );
        let holding = TodayMove {
            change: None,
            ..day(-2.1, 0.0, &[])
        };
        // Before Friday's open, the latest session is Thursday's.
        let friday = clock("2026-10-09T12:00:00Z");
        assert_eq!(
            written(&super::lines(&holding, Some("NVDA"), Some(-196.2), &friday)[0]),
            "NVDA fell 2.1% (\u{2212}$196) on Thursday, more than the S&P 500\u{2019}s 0.7%."
        );
        // Close to the market's move reads as in line with it.
        let close = day(-0.8, -100.0, &[("NVDA", -1.0, -100.0)]);
        assert_eq!(
            written(&super::lines(&close, None, None, &friday)[0]),
            "Your investments fell 0.8% (\u{2212}$100) on Thursday, in line with the S&P 500."
        );
    }
}
