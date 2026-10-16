//! The six families' events for one session, each built from the session's own data.
use super::*;
use crate::providers::CorporateAction;

/// The ledger's events in `day`: trades and the cash activity of investment accounts.
pub(super) fn ledger(day: NaiveDate, sources: &Sources) -> Vec<Event> {
    let mut events = Vec::new();
    for trade in sources.trades {
        if !sources.accounts.contains(&trade.account_id) || session_of(&trade.date) != Some(day) {
            continue;
        }
        let upper = trade.r#type.to_ascii_uppercase();
        let kind = if upper.contains("REINVEST") {
            "reinvestment"
        } else if upper.contains("BUY") {
            "buy"
        } else if upper.contains("SELL") {
            "sell"
        } else {
            continue;
        };
        let mut item = event(
            format!("ledger:{}", trade.id),
            Family::Ledger,
            kind,
            day,
            sources,
        );
        item.symbol = trade.ticker.clone();
        item.account_id = Some(trade.account_id.clone());
        item.at = Some(trade.date.clone());
        item.quantities = json!({
            "units": trade.units, "price": trade.price, "amount": trade.amount,
            "realizedGain": trade.estimated_realized_gain,
        });
        item.effect = Some(round2(trade.amount));
        item.source = Some(EventSource {
            provider: "snapshot".into(),
            provider_id: Some(trade.id.clone()),
            url: None,
            title: trade.description.clone().unwrap_or_default(),
            publisher: trade.account.clone(),
            published_at: None,
            input_class: InputClass::Red,
        });
        events.push(item);
    }
    for transaction in sources.transactions {
        let date = transaction
            .posted_on
            .as_deref()
            .unwrap_or(&transaction.date);
        let Some(account) = transaction
            .account_id
            .as_ref()
            .filter(|id| sources.accounts.contains(*id))
        else {
            continue;
        };
        if transaction.pending || session_of(date) != Some(day) {
            continue;
        }
        use crate::transaction_policy::Kind;
        let kind = match transaction.classification.as_ref().map(|c| &c.kind) {
            Some(Kind::Dividend) => "dividend",
            Some(Kind::Interest) => "interest",
            Some(Kind::Fee) => "fee",
            Some(Kind::Tax) => "tax",
            Some(Kind::Transfer) if transaction.amount > 0.0 => "deposit",
            Some(Kind::Transfer) if transaction.amount < 0.0 => "withdrawal",
            _ => continue,
        };
        let mut item = event(
            format!("ledger:{}", transaction.id),
            Family::Ledger,
            kind,
            day,
            sources,
        );
        // Dividends name their security after a middle dot, as the classifier writes them.
        item.symbol = transaction
            .merchant
            .rsplit_once(" · ")
            .map(|(_, ticker)| ticker.to_owned())
            .filter(|ticker| sources.held.contains(ticker));
        item.account_id = Some(account.clone());
        item.at = Some(date.to_owned());
        item.quantities = json!({ "amount": transaction.amount });
        item.effect = Some(round2(transaction.amount));
        item.dates.payable = (kind == "dividend").then(|| date.to_owned());
        events.push(item);
    }
    events
}

/// The event kind a corporate action is listed as.
pub(in crate::summary) fn action_kind(action: &CorporateAction) -> &'static str {
    match action {
        CorporateAction::Split { .. } => "split",
        CorporateAction::Dividend { .. } => "cashDividend",
    }
}

/// What a corporate action pays on the shares held at `held_on`. A split changes shares and
/// price together, so it is never worth anything; a dividend on no shares is unknown.
pub(in crate::summary) fn action_dollars(
    action: &CorporateAction,
    shares: &BTreeMap<String, Shares>,
    held_on: NaiveDate,
) -> Option<f64> {
    match action {
        CorporateAction::Split { .. } => Some(0.0),
        CorporateAction::Dividend { symbol, rate, .. } => {
            let held = shares.get(symbol)?.at(held_on);
            (held > 0.0).then(|| round2(rate * held))
        }
    }
}

/// Corporate actions whose ex-date falls on `day`, for held securities.
pub(super) fn corporate_actions(day: NaiveDate, sources: &Sources) -> Vec<Event> {
    sources
        .corporate_actions
        .iter()
        .filter_map(|action| {
            let symbol = action.symbol();
            let ex = action.ex_date().to_string();
            if !sources.held.contains(symbol) || session_of(&ex) != Some(day) {
                return None;
            }
            let kind = action_kind(action);
            let mut item = event(
                format!("action:{kind}:{symbol}:{ex}"),
                Family::CorporateAction,
                kind,
                day,
                sources,
            );
            item.symbol = Some(symbol.into());
            item.at = Some(ex.clone());
            item.dates.ex = Some(ex);
            if let CorporateAction::Dividend { payable_date, .. } = action {
                item.dates.payable = payable_date.map(|date| date.to_string());
            }
            item.quantities = serde_json::to_value(action).unwrap_or(Value::Null);
            item.effect = day
                .pred_opt()
                .and_then(|before| action_dollars(action, sources.shares, before));
            item.source = Some(EventSource {
                provider: "alpaca".into(),
                provider_id: None,
                url: None,
                title: match action {
                    CorporateAction::Split { .. } => format!("{symbol} split"),
                    CorporateAction::Dividend { .. } => format!("{symbol} dividend"),
                },
                publisher: "Alpaca".into(),
                published_at: None,
                input_class: InputClass::Green,
            });
            Some(item)
        })
        .collect()
}

/// Current and periodic reports priced in `day`.
pub(super) fn disclosures(day: NaiveDate, sources: &Sources) -> Vec<Event> {
    let mut events = Vec::new();
    for holding in sources.evidence {
        for filing in &holding.filings {
            if session_of(&filing.filed_at) != Some(day) {
                continue;
            }
            let key = sec::accession(filing).unwrap_or(&filing.url).to_owned();
            let mut item = event(
                format!("filing:{key}"),
                Family::Disclosure,
                &filing.form,
                day,
                sources,
            );
            item.subtype = (!filing.items.is_empty()).then(|| {
                filing
                    .items
                    .iter()
                    .map(|item| item.code.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            });
            item.symbol = Some(holding.ticker.clone());
            item.at = Some(filing.filed_at.clone());
            item.precision = Precision::Exact;
            item.dates.filed = Some(filing.filed_at.clone());
            item.z = holding
                .own
                .get(&day)
                .map(|(own, sigma)| round2(own / sigma));
            item.source = Some(EventSource {
                provider: "sec".into(),
                provider_id: Some(key),
                url: Some(filing.url.clone()),
                title: sec::filing_title(filing),
                publisher: format!("SEC {}", filing.form),
                published_at: Some(filing.filed_at.clone()),
                input_class: InputClass::Green,
            });
            events.push(item);
        }
    }
    events
}

/// The reviewed calendar of scheduled releases.
#[derive(Deserialize)]
struct Calendar {
    fomc: Dates,
    releases: Releases,
}

#[derive(Deserialize)]
struct Dates {
    source: String,
    decisions: Vec<NaiveDate>,
}

#[derive(Deserialize)]
struct Releases {
    source: String,
    cpi: Vec<NaiveDate>,
    employment: Vec<NaiveDate>,
}

static CALENDAR: LazyLock<Calendar> = LazyLock::new(|| {
    serde_json::from_str(include_str!("macro-calendar.json")).expect("the macro calendar is valid")
});

/// The S&P 500's quarterly rebalance takes effect before the open on the Monday after the
/// third Friday of March, June, September, and December.
pub(in crate::summary) fn rebalance(day: NaiveDate) -> bool {
    use chrono::Datelike;
    matches!(day.month(), 3 | 6 | 9 | 12)
        && day.weekday() == chrono::Weekday::Mon
        && crate::insights::calendar::nth_weekday(day.year(), day.month(), chrono::Weekday::Fri, 3)
            + Duration::days(3)
            == day
}

/// Scheduled market-wide events on `day`: one each for the portfolio, never per holding.
pub(in crate::summary) fn macro_events(day: NaiveDate, sources: &Sources) -> Vec<Event> {
    let calendar = &*CALENDAR;
    let mut found: Vec<(&str, &str, &str, &str, Option<String>)> = Vec::new();
    if calendar.fomc.decisions.contains(&day) {
        found.push((
            "fomc",
            "Federal Reserve rate decision",
            "federalReserve",
            &calendar.fomc.source,
            Some(
                session::at_new_york(day, NaiveTime::from_hms_opt(14, 0, 0).expect("time"))
                    .to_rfc3339(),
            ),
        ));
    }
    for (kind, title, dates) in [
        ("cpi", "Consumer price index", &calendar.releases.cpi),
        ("employment", "Jobs report", &calendar.releases.employment),
    ] {
        if dates.contains(&day) {
            found.push((
                kind,
                title,
                "bls",
                &calendar.releases.source,
                Some(
                    session::at_new_york(day, NaiveTime::from_hms_opt(8, 30, 0).expect("time"))
                        .to_rfc3339(),
                ),
            ));
        }
    }
    if session::closes_early(day) {
        found.push((
            "earlyClose",
            "Market closes at 1 p.m.",
            "nyse",
            "https://www.nyse.com/markets/hours-calendars",
            None,
        ));
    }
    if rebalance(day) {
        found.push((
            "indexRebalance",
            "S&P 500 quarterly rebalance",
            "spdji",
            "https://www.spglobal.com/spdji/en/governance/index-policies/",
            None,
        ));
    }
    found
        .into_iter()
        .map(|(kind, title, provider, url, at)| {
            let mut item = event(
                format!("macro:{kind}:{day}"),
                Family::Macro,
                kind,
                day,
                sources,
            );
            item.precision = if at.is_some() {
                Precision::Exact
            } else {
                Precision::Date
            };
            item.at = at;
            item.source = Some(EventSource {
                provider: provider.into(),
                provider_id: None,
                url: Some(url.into()),
                title: title.into(),
                publisher: title.into(),
                published_at: None,
                input_class: InputClass::Green,
            });
            item
        })
        .collect()
}

/// Every scheduled market-wide event and market holiday after `from` through `to`, for what's
/// ahead.
pub(in crate::summary) fn scheduled_macro(
    from: NaiveDate,
    to: NaiveDate,
    sources: &Sources,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut day = from.succ_opt().unwrap_or(from);
    while day <= to {
        if session::is_trading_day(day) {
            events.extend(macro_events(day, sources).into_iter().map(|mut item| {
                item.status = Status::Scheduled;
                item
            }));
        } else if session::is_market_holiday(day) {
            let mut item = event(
                format!("macro:holiday:{day}"),
                Family::Macro,
                "holiday",
                Session::on_or_after(day).date,
                sources,
            );
            item.at = Some(day.to_string());
            item.status = Status::Scheduled;
            item.source = Some(EventSource {
                provider: "nyse".into(),
                provider_id: None,
                url: Some("https://www.nyse.com/markets/hours-calendars".into()),
                title: "Market closed for a holiday".into(),
                publisher: "NYSE".into(),
                published_at: None,
                input_class: InputClass::Green,
            });
            events.push(item);
        }
        day = day.succ_opt().unwrap_or(day);
    }
    events
}

/// Stories priced in `day`: each held company's relevant ones and the market's.
pub(super) fn news(day: NaiveDate, sources: &Sources) -> Vec<Event> {
    let mut events = Vec::new();
    let mut seen = BTreeSet::new();
    let stories = sources
        .evidence
        .iter()
        .flat_map(|holding| {
            holding
                .articles
                .iter()
                .filter(|story| story.scope == NewsScope::Company && story.relevance >= RELEVANT)
                .map(move |story| (Some(holding.ticker.as_str()), story))
        })
        .chain(sources.market_news.iter().map(|story| (None, story)));
    for (symbol, story) in stories {
        if session_of(&story.created_at) != Some(day) || !seen.insert(story.url.clone()) {
            continue;
        }
        let mut item = event(news_id(&story.url), Family::News, "story", day, sources);
        item.subtype = story_kind(story).map(str::to_owned);
        item.symbol = symbol.map(str::to_owned);
        item.at = Some(story.created_at.clone());
        item.source = Some(EventSource {
            provider: if story.source.to_lowercase().contains("cnbc") {
                "cnbc".into()
            } else {
                "alpaca".into()
            },
            provider_id: None,
            url: Some(story.url.clone()),
            title: story.headline.clone(),
            publisher: story.source.clone(),
            published_at: Some(story.created_at.clone()),
            input_class: input_class(&story.source),
        });
        item.quantities = json!({ "relevance": story.relevance, "symbols": story.symbols });
        item.z = symbol
            .and_then(|symbol| {
                sources
                    .evidence
                    .iter()
                    .find(|holding| holding.ticker == symbol)
            })
            .and_then(|holding| holding.own.get(&day))
            .map(|(own, sigma)| round2(own / sigma));
        events.push(item);
    }
    events
}

/// What a story reports, from its tags and headline: the category it shares with a filing or
/// action, or `explainer`, `roundup`, or `opinion`, which are never causes.
pub(super) fn story_kind(story: &MarketNewsArticle) -> Option<&'static str> {
    if headlines::filler(&story.headline) || story.tags.contains(&NewsTag::Explainer) {
        return Some("explainer");
    }
    if story.url.to_ascii_lowercase().contains("/opinion/") {
        return Some("opinion");
    }
    if headlines::is_roundup(&story.headline) {
        return Some("roundup");
    }
    let has = |tag: NewsTag| story.tags.contains(&tag);
    let lower = story.headline.to_lowercase();
    if has(NewsTag::Earnings) || has(NewsTag::Guidance) {
        Some("earnings")
    } else if lower.contains("dividend") {
        Some("dividend")
    } else if lower.contains("split") {
        Some("split")
    } else if has(NewsTag::Deal) {
        Some("deal")
    } else if has(NewsTag::Leadership) {
        Some("leadership")
    } else if has(NewsTag::Legal) {
        Some("legal")
    } else if has(NewsTag::Product) {
        Some("product")
    } else if has(NewsTag::Capital) {
        Some("capital")
    } else {
        None
    }
}

/// Observed flags for each held security in `day`, from its own move and its closes.
pub(super) fn statistical(
    day: NaiveDate,
    record: Option<&SessionRecord>,
    sources: &Sources,
) -> Vec<Event> {
    let mut events = Vec::new();
    for symbol in sources.held {
        let row =
            record.and_then(|record| record.holdings.iter().find(|row| &row.ticker == symbol));
        let mut flag = |kind: &str, subtype: Option<String>, quantities: Value| {
            let mut item = event(
                format!(
                    "stat:{symbol}:{kind}:{}:{day}",
                    subtype.as_deref().unwrap_or("")
                ),
                Family::Statistical,
                kind,
                day,
                sources,
            );
            item.symbol = Some(symbol.clone());
            item.subtype = subtype;
            item.quantities = quantities;
            item.z = row.and_then(|row| row.own_z);
            item.effect = row.and_then(|row| row.own);
            item.source = Some(EventSource {
                provider: "brief".into(),
                provider_id: None,
                url: None,
                title: String::new(),
                publisher: "Brief".into(),
                published_at: None,
                input_class: InputClass::Green,
            });
            events.push(item);
        };
        if let Some((z, own)) = row.and_then(|row| Some((row.own_z?, row.own_pct))) {
            if z.abs() >= ABNORMAL_Z {
                flag(
                    "abnormalReturn",
                    Some(if z < 0.0 { "down" } else { "up" }.into()),
                    json!({ "ownPct": own, "z": z }),
                );
            }
        }
        if let Some(ratio) = volume_ratio(sources.volumes.get(symbol), day) {
            if ratio >= VOLUME_RATIO {
                flag("unusualVolume", None, json!({ "ratio": round2(ratio) }));
            }
        }
        let Some(series) = sources.closes.get(symbol) else {
            continue;
        };
        let upto = series.partition_point(|(date, _)| *date <= day);
        let Some(&(last, close)) = upto.checked_sub(1).and_then(|index| series.get(index)) else {
            continue;
        };
        if last != day || upto < MIN_YEAR_SESSIONS + 1 {
            continue;
        }
        let prior = &series[upto.saturating_sub(YEAR_SESSIONS)..upto - 1];
        let high = prior
            .iter()
            .map(|(_, close)| *close)
            .fold(f64::MIN, f64::max);
        let low = prior
            .iter()
            .map(|(_, close)| *close)
            .fold(f64::MAX, f64::min);
        if close > high {
            flag(
                "newHigh",
                None,
                json!({ "close": close, "previousHigh": high }),
            );
        } else if close < low {
            flag(
                "newLow",
                None,
                json!({ "close": close, "previousLow": low }),
            );
        }
        // A fall from the 52-week high crossing a round mark this session.
        let peak = high.max(close);
        let before = prior.last().map(|(_, close)| *close);
        let peak_before = prior[..prior.len().saturating_sub(1)]
            .iter()
            .map(|(_, close)| *close)
            .fold(f64::MIN, f64::max)
            .max(before.unwrap_or(f64::MIN));
        if let Some(before) = before {
            let now_down = (1.0 - close / peak) * 100.0;
            let then_down = (1.0 - before / peak_before) * 100.0;
            for mark in DRAWDOWNS {
                if now_down >= mark && then_down < mark {
                    flag(
                        "drawdown",
                        Some(format!("{mark}")),
                        json!({ "fromHighPct": round2(-now_down) }),
                    );
                }
            }
        }
    }
    events
}

/// Volume on `day` against its median over the sessions before.
pub(in crate::summary) fn volume_ratio(
    volumes: Option<&BTreeMap<NaiveDate, f64>>,
    day: NaiveDate,
) -> Option<f64> {
    let volumes = volumes?;
    let today = *volumes.get(&day)?;
    let mut prior: Vec<f64> = volumes
        .range(..day)
        .rev()
        .take(VOLUME_SESSIONS)
        .map(|(_, volume)| *volume)
        .collect();
    if prior.len() < VOLUME_SESSIONS {
        return None;
    }
    let median = crate::numeric::median(&mut prior)?;
    (median > 0.0).then(|| today / median)
}
