//! Benefit windows: each period's credits and purchases set against its allowance, and how
//! each closed window ended.
use super::*;

impl RuleSet {
    /// Credits against the annual fee since it last posted, or since January (or the start of
    /// the saved history, when later) when the history holds no fee charge from the past year,
    /// and what the benefit periods plan to credit before the year ends.
    pub fn fee_recovery(
        &self,
        transactions: &[&Transaction],
        activity: &[BenefitActivity],
        history: &[BenefitHistory],
        today: NaiveDate,
    ) -> Option<FeeRecovery> {
        let annual_fee = self.annual_fee?;
        let charged = self.annual_fee_matches.as_ref().and_then(|matches| {
            transactions
                .iter()
                .filter(|transaction| !transaction.pending && transaction.amount < 0.0)
                .filter(|transaction| {
                    matches.is_match(&format!(
                        "{} {}",
                        transaction.merchant,
                        transaction.description.as_deref().unwrap_or("")
                    ))
                })
                .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
                .filter(|date| *date <= today && (today - *date).num_days() <= 366)
                .max()
        });
        let history_start = transactions
            .iter()
            .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
            .min();
        let january = NaiveDate::from_ymd_opt(today.year(), 1, 1)?;
        let (since, basis) = match (charged, history_start) {
            (Some(date), _) => (date, "fee-charge"),
            (None, Some(start)) if start > january => (start, "card-history"),
            (None, _) => (january, "calendar-year"),
        };
        let others = transactions
            .iter()
            .filter(|transaction| transaction.other)
            .map(|transaction| transaction.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let (mut recovered, mut for_others) = (0, 0);
        for item in activity
            .iter()
            .filter(|item| item.is_posted_credit() && item.date >= since && item.date <= today)
        {
            if others.contains(item.id.as_str()) {
                for_others += minor_units(item.amount);
            } else {
                recovered += minor_units(item.amount);
            }
        }
        // A year after the fee posted; without one, the anniversary is unknown, so the calendar
        // year's end stands in for it.
        let through = match basis {
            "fee-charge" => since
                .checked_add_months(chrono::Months::new(12))
                .map(|renewal| renewal - Duration::days(1))?,
            _ => NaiveDate::from_ymd_opt(today.year(), 12, 31)?,
        };
        // A purchase made for someone else earns them the credit, not you.
        let expected = history
            .iter()
            .flat_map(|benefit| &benefit.periods)
            .flat_map(|period| &period.planned)
            .filter(|credit| credit.date <= through)
            .filter(|credit| {
                credit
                    .transaction_id
                    .as_deref()
                    .is_none_or(|id| !others.contains(id))
            })
            .map(|credit| minor_units(credit.amount))
            .sum::<i64>();
        Some(FeeRecovery {
            annual_fee,
            recovered: from_minor(recovered.max(0)),
            for_others: from_minor(for_others.max(0)),
            since,
            basis,
            through,
            expected: from_minor(expected.max(0)),
        })
    }
}

/// Credit totals for one window, in cents.
pub(super) struct WindowCredits<'a> {
    /// Credits and reversals that certainly count toward the window.
    pub(super) certain: i64,
    /// Credits and reversals posted in this window that may count toward the previous one.
    pub(super) provisional: i64,
    /// Credits and reversals that count toward this window or an adjacent one.
    pub(super) uncertain_low: i64,
    pub(super) uncertain_high: i64,
    /// Credits that qualifying purchases in the window are still waiting for.
    pub(super) awaiting: i64,
    /// Everything shown as the window's evidence.
    pub(super) activity: Vec<&'a BenefitActivity>,
}

impl WindowCredits<'_> {
    fn uncertain(&self) -> bool {
        self.uncertain_low != 0 || self.uncertain_high != 0
    }

    /// The unused allowance, as a range, before applying the smallest qualifying charge.
    pub(super) fn unused(&self, cap: i64) -> (i64, i64) {
        let clamp = |value: i64| value.clamp(0, cap);
        (
            clamp(cap - self.certain - self.uncertain_high),
            clamp(cap - self.certain - self.uncertain_low),
        )
    }
}

impl RuleSet {
    /// Whether the text is an issuer statement-credit descriptor, such as "Platinum Hotel
    /// Credit". Plaid files these under the benefit's spending category, a transfer, or a
    /// fee, so the descriptor decides that they are credits.
    pub fn is_issuer_credit(&self, text: &str) -> bool {
        !REFUND.is_match(text)
            && (ISSUER_CREDIT.is_match(text)
                || self.rules.iter().any(|rule| {
                    rule.data.issuer_credits
                        && rule
                            .credit_matches
                            .as_ref()
                            .is_some_and(|re| re.is_match(text))
                }))
    }

    pub fn options(&self) -> Vec<BenefitOption> {
        self.rules
            .iter()
            .filter(|rule| rule.data.effective_until.is_none())
            .map(|rule| BenefitOption {
                id: rule.data.id.clone(),
                name: rule.data.name.clone(),
            })
            .collect()
    }

    /// Classifies posted and pending card activity against the benefit rules, then pairs
    /// credits with purchases and places each credit in a window, as of `reference`.
    pub fn activity(&self, transactions: &[&Transaction], reference: &str) -> Vec<BenefitActivity> {
        let reference_day = calendar::transaction_date(reference);
        let mut activity = transactions
            .iter()
            .filter_map(|transaction| {
                let posted = calendar::transaction_date(transaction.posted_date())?;
                let date = calendar::transaction_date(transaction.occurred_date())?;
                if reference_day.is_none_or(|day| posted > day)
                    || !transaction.amount.is_finite()
                    || transaction.amount == 0.0
                {
                    return None;
                }
                self.classify(transaction, date, posted)
            })
            .collect::<Vec<_>>();
        activity.sort_by_key(|item| std::cmp::Reverse(item.date));
        if let Some(today) = reference_day {
            for rule in &self.rules {
                attribution::attribute(rule, &mut activity, today);
            }
        }
        activity
    }

    fn classify(
        &self,
        transaction: &Transaction,
        date: NaiveDate,
        posted: NaiveDate,
    ) -> Option<BenefitActivity> {
        // Rejected by hand: neither a credit nor a qualifying purchase for any benefit.
        if transaction.benefit_confirmed == Some(false) {
            return None;
        }
        let text = transaction.descriptors();
        // A benefit chosen by hand wins over the descriptor rules.
        let assigned = transaction
            .benefit_id
            .as_deref()
            .and_then(|id| self.rules.iter().find(|rule| rule.data.id == id));
        let rule = assigned.or_else(|| self.matching_rule(transaction, &text, date))?;
        let worded = rule
            .credit_matches
            .as_ref()
            .is_some_and(|re| re.is_match(&text))
            && !REFUND.is_match(&text);
        // Confirming or assigning makes a positive amount a credit; only the issuer's wording
        // makes a charge a reversal.
        let identified_credit = rule.data.issuer_credits
            && if transaction.amount > 0.0 {
                worded || assigned.is_some() || transaction.benefit_confirmed == Some(true)
            } else {
                worded
            };
        let kind = match (identified_credit, transaction.amount > 0.0) {
            (true, true) => ActivityKind::Credit,
            (true, false) => ActivityKind::Reversal,
            (false, false) => ActivityKind::Purchase,
            (false, true) => ActivityKind::Unverified,
        };
        // A hand assignment also counts a purchase that posted after the benefit retired.
        let in_effect = rule.data.effective_from.is_none_or(|from| date >= from)
            && (rule.data.effective_until.is_none_or(|until| date <= until)
                || identified_credit
                || assigned.is_some());
        in_effect.then(|| BenefitActivity {
            id: transaction.id.clone(),
            merchant: transaction.merchant.clone(),
            description: transaction.description.clone(),
            amount: transaction.amount,
            pending: transaction.pending,
            account_id: transaction.account_id.clone(),
            date,
            posted,
            dated: transaction.occurred_on.is_some(),
            benefit_id: rule.data.id.clone(),
            benefit_name: rule.data.name.clone(),
            kind,
            paired_id: None,
            credit_window: None,
            alternate_credit_window: None,
            credit: None,
        })
    }

    /// The first rule whose descriptors match, unless the rule excludes this transaction.
    fn matching_rule(
        &self,
        transaction: &Transaction,
        text: &str,
        date: NaiveDate,
    ) -> Option<&Rule> {
        self.rules.iter().find(|rule| {
            let excluded = rule.excludes.as_ref().is_some_and(|re| re.is_match(text))
                || merchant_amount(&rule.data.exclude_merchant_amounts, transaction)
                || rule
                    .requires_before
                    .as_ref()
                    .is_some_and(|(re, before)| date < *before && !re.is_match(text));
            !excluded
                && (rule.matches.is_match(text)
                    || rule
                        .credit_matches
                        .as_ref()
                        .is_some_and(|re| re.is_match(text))
                    || merchant_amount(&rule.data.merchant_amount_matches, transaction))
        })
    }

    /// The benefit a charge qualifies for as a purchase, if any: a recurring series of such
    /// charges is expected to earn that benefit's credits.
    pub fn purchase_benefit(&self, transaction: &Transaction) -> Option<String> {
        let date = calendar::transaction_date(transaction.occurred_date())?;
        let posted = calendar::transaction_date(transaction.posted_date())?;
        self.classify(transaction, date, posted)
            .filter(|activity| activity.kind == ActivityKind::Purchase)
            .map(|activity| activity.benefit_id)
    }

    /// The benefit a positive transaction is an identified credit for, if any.
    pub fn credit_identity(&self, transaction: &Transaction) -> Option<(String, String)> {
        self.activity(&[transaction], transaction.posted_date())
            .into_iter()
            .next()
            .filter(|activity| activity.kind == ActivityKind::Credit)
            .map(|activity| (activity.benefit_id, activity.benefit_name))
    }

    /// Current benefit windows as of `reference`, using activity saved as of `snapshot`.
    #[cfg(test)]
    pub fn tracker(
        &self,
        transactions: &[&Transaction],
        reference: &str,
        snapshot: &str,
        zone: &dyn Zone,
    ) -> Vec<BenefitWindow> {
        let matched = self.activity(transactions, snapshot);
        self.tracker_for(&matched, reference, snapshot, None, zone)
    }

    /// `tracker` over activity already classified as of `snapshot`. `history_start` is the
    /// first day of the saved card history, when a window may begin before it.
    pub fn tracker_for(
        &self,
        matched: &[BenefitActivity],
        reference: &str,
        snapshot: &str,
        history_start: Option<NaiveDate>,
        zone: &dyn Zone,
    ) -> Vec<BenefitWindow> {
        let Some(reference_instant) = calendar::parse_instant(reference) else {
            return Vec::new();
        };
        let local_reference = zone.date(reference_instant);
        let snapshot_instant = calendar::parse_instant(snapshot);
        let mut windows = self
            .rules
            .iter()
            .filter(|rule| {
                rule.data
                    .effective_from
                    .is_none_or(|from| from <= local_reference)
                    && rule
                        .data
                        .effective_until
                        .is_none_or(|until| until >= local_reference)
            })
            .map(|rule| {
                self.window(
                    rule,
                    matched,
                    reference_instant,
                    snapshot_instant,
                    history_start,
                    zone,
                )
            })
            .collect::<Vec<_>>();
        windows.sort_by(|left, right| {
            (!left.cadence.is_calendar())
                .cmp(&!right.cadence.is_calendar())
                .then_with(|| left.window_end.cmp(&right.window_end))
                .then_with(|| collate(&left.name, &right.name))
        });
        windows
    }

    /// Credit totals for one window of a rule. `through` excludes activity posted later.
    pub(super) fn window_credits<'a>(
        rule: &Rule,
        period: &Period,
        matched: &'a [BenefitActivity],
        through: NaiveDate,
    ) -> WindowCredits<'a> {
        let mut credits = WindowCredits {
            certain: 0,
            provisional: 0,
            uncertain_low: 0,
            uncertain_high: 0,
            awaiting: 0,
            activity: Vec::new(),
        };
        for item in matched
            .iter()
            .filter(|item| item.benefit_id == rule.data.id && item.date <= through)
        {
            let shown = if item.is_posted_credit() && period.cadence == Cadence::Renewal {
                // A renewal benefit is judged by its latest credit, whenever it posted.
                credits.certain += minor_units(item.amount);
                true
            } else if item.is_posted_credit() {
                let amount = minor_units(item.amount);
                let later = item.credit_window == Some(period.start);
                let earlier = item.alternate_credit_window == Some(period.start);
                match (later, item.alternate_credit_window.is_some()) {
                    (true, false) => credits.certain += amount,
                    (true, true) => {
                        credits.provisional += amount;
                        credits.uncertain_low += amount.min(0);
                        credits.uncertain_high += amount.max(0);
                    }
                    (false, _) if earlier => {
                        credits.uncertain_low += amount.min(0);
                        credits.uncertain_high += amount.max(0);
                    }
                    _ => {}
                }
                later || earlier
            } else {
                let inside = item.date >= period.start && item.date <= period.end;
                if inside {
                    if let Some(
                        PurchaseCredit::Expected { amount, .. }
                        | PurchaseCredit::Late { amount, .. },
                    ) = item.credit
                    {
                        credits.awaiting += minor_units(amount);
                    }
                }
                inside
            };
            if shown || period.cadence == Cadence::Renewal {
                credits.activity.push(item);
            }
        }
        credits
    }

    fn window(
        &self,
        rule: &Rule,
        matched: &[BenefitActivity],
        reference: DateTime<Utc>,
        snapshot: Option<DateTime<Utc>>,
        history_start: Option<NaiveDate>,
        zone: &dyn Zone,
    ) -> BenefitWindow {
        let rule_zone = rule.zone(zone);
        let rule_reference = rule_zone.date(reference);
        let definition = &rule.data;
        let period = rule.period(rule_reference);
        let credits = Self::window_credits(rule, &period, matched, rule_reference);
        let cap = minor_units(period.cap);
        let lag = CreditLag::observe(rule, matched);
        let ours = || {
            matched
                .iter()
                .filter(|item| item.benefit_id == definition.id && item.date <= rule_reference)
        };
        // `matched` is newest first, so the first posted credit is the latest one.
        let last_credit = ours().find(|item| item.kind == ActivityKind::Credit && !item.pending);
        let estimated_credit_amount = definition.estimate_from_purchases.then(|| {
            if credits
                .activity
                .iter()
                .any(|item| item.kind == ActivityKind::Purchase && !item.pending)
            {
                period.cap
            } else {
                0.0
            }
        });
        let snapshot_before_window = snapshot
            .map(|instant| rule_zone.date(instant) < period.start)
            .unwrap_or(false);
        let tracked = !snapshot_before_window
            && definition.track_remaining
            && definition.issuer_credits
            && period.cadence.is_calendar();
        // A window the history reaches partway is judged by the credits it shows; the date
        // says where that history starts.
        let history_from =
            history_start.filter(|start| period.cadence.is_calendar() && *start > period.start);
        let (unused_low, unused_high) = credits.unused(cap);
        // A remainder under the smallest charge, or only cents, can't be spent.
        let remaining_range = tracked.then(|| AmountRange {
            low: from_minor(rule.spendable(unused_low)),
            high: from_minor(rule.spendable(unused_high)),
        });
        let unusable_remainder = (tracked && !credits.uncertain())
            .then(|| unused_low - rule.spendable(unused_low))
            .filter(|remainder| *remainder > 0)
            .map(from_minor);
        let remaining_amount = remaining_range
            .filter(|range| range.low == range.high)
            .map(|range| range.low);
        let renewal_credit = last_credit.filter(|last| {
            period.cadence == Cadence::Renewal
                && !ours().any(|item| {
                    item.kind == ActivityKind::Reversal && !item.pending && item.date >= last.date
                })
        });
        // Global Entry's $120 credit renews after four years; TSA PreCheck's, up to $85, after
        // four and a half.
        let renewal_check = renewal_credit.map(|credit| {
            let months = if credit.amount > 85.0 { 48 } else { 54 };
            calendar::add_months_first(credit.date, months)
                + Duration::days(credit.date.day() as i64 - 1)
        });
        let credited_amount = from_minor(credits.certain + credits.provisional);
        let status = if !definition.issuer_credits {
            "external"
        } else if credited_amount > 0.0 {
            "credited"
        } else if credits
            .activity
            .iter()
            .any(|item| item.kind == ActivityKind::Purchase)
        {
            "matched"
        } else {
            "unused"
        };
        let last_credit_outside = last_credit
            .filter(|last| !credits.activity.iter().any(|item| item == last))
            .map(|last| LastCredit {
                amount: last.amount,
                date: calendar::long_label(last.date),
            });
        BenefitWindow {
            id: definition.id.clone(),
            name: definition.name.clone(),
            limit: period.limit.clone(),
            cap: period.cap,
            cadence: period.cadence,
            instruction: definition.instruction.clone(),
            url: definition.url.clone(),
            deadline_note: definition.deadline_note.clone(),
            period_uncertain: credits.uncertain(),
            snapshot_before_window,
            posting_delay_days: definition.posting_delay_days,
            credit_lag_days: lag.days,
            credit_lag_learned: lag.learned,
            renewal_check: renewal_check.map(calendar::long_label),
            status,
            recurring: definition.recurring,
            history_from,
            credited_amount,
            uncertain_credit_amount: from_minor(credits.uncertain_low + credits.uncertain_high),
            awaiting_credit_amount: from_minor(credits.awaiting.min(unused_high)),
            estimated_credit_amount,
            remaining_amount,
            remaining_range,
            unusable_remainder,
            activity: credits.activity.into_iter().cloned().collect(),
            window_start: period.start,
            window_end: period.end,
            window_label: format!(
                "{} – {}, {}",
                calendar::short_label(period.start),
                calendar::short_label(period.end),
                period.end.year()
            ),
            days_remaining: (period.end - rule_reference).num_days().max(0),
            last_credit: last_credit_outside,
            reset: calendar::short_label(period.end),
        }
    }
}

/// Alphabetical order that ignores case first, like the renderer's locale collation.
pub(super) fn collate(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| right.cmp(left))
}

/// JavaScript `Math.round`: halves round toward positive infinity.
pub(super) fn benefit_window(date: NaiveDate, cadence: Cadence) -> (NaiveDate, NaiveDate) {
    let month = date.month0() as i64;
    let (start_month, duration) = match cadence {
        Cadence::Monthly => (month, 1),
        Cadence::Quarterly => (month / 3 * 3, 3),
        Cadence::Semiannual => (month / 6 * 6, 6),
        _ => (0, 12),
    };
    let start = calendar::first_of_month(date.year(), start_month + 1);
    let end = calendar::first_of_month(date.year(), start_month + duration + 1)
        .pred_opt()
        .expect("date in range");
    (start, end)
}
