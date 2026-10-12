//! Benefit windows: each period's credits and purchases set against its allowance, and how
//! each closed window ended.
use super::*;

impl RuleSet {
    /// Credits against the annual fee since it last posted, or since January when the saved
    /// history holds no fee charge from the past year.
    pub fn fee_recovery(
        &self,
        transactions: &[&Transaction],
        activity: &[BenefitActivity],
        hidden_benefit_ids: &[String],
        today: NaiveDate,
        reference: &str,
        zone: &dyn Zone,
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
                .filter_map(|transaction| {
                    calendar::transaction_date(transaction.posted_date(), reference, zone)
                })
                .filter(|date| *date <= today && (today - *date).num_days() <= 366)
                .max()
        });
        let (since, basis) = match charged {
            Some(date) => (date, "fee-charge"),
            None => (
                NaiveDate::from_ymd_opt(today.year(), 1, 1)?,
                "calendar-year",
            ),
        };
        let others = transactions
            .iter()
            .filter(|transaction| transaction.other)
            .map(|transaction| transaction.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let (mut recovered, mut for_others) = (0, 0);
        for item in activity.iter().filter(|item| {
            item.is_posted_credit()
                && item.date >= since
                && item.date <= today
                && !hidden_benefit_ids.contains(&item.benefit_id)
        }) {
            if others.contains(item.id.as_str()) {
                for_others += minor_units(item.amount);
            } else {
                recovered += minor_units(item.amount);
            }
        }
        Some(FeeRecovery {
            annual_fee,
            recovered: from_minor(recovered.max(0)),
            for_others: from_minor(for_others.max(0)),
            since,
            basis,
        })
    }
}

/// Credit totals for one window, in cents.
struct WindowCredits<'a> {
    /// Credits and reversals that certainly count toward the window.
    certain: i64,
    /// Credits and reversals posted in this window that may count toward the previous one.
    provisional: i64,
    /// Credits and reversals that count toward this window or an adjacent one.
    uncertain_low: i64,
    uncertain_high: i64,
    /// Credits that qualifying purchases in the window are still waiting for.
    awaiting: i64,
    /// Everything shown as the window's evidence.
    activity: Vec<&'a BenefitActivity>,
}

impl WindowCredits<'_> {
    fn uncertain(&self) -> bool {
        self.uncertain_low != 0 || self.uncertain_high != 0
    }

    /// The unused allowance, as a range, before applying the smallest qualifying charge.
    fn unused(&self, cap: i64) -> (i64, i64) {
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
    pub fn activity(
        &self,
        transactions: &[&Transaction],
        reference: &str,
        zone: &dyn Zone,
    ) -> Vec<BenefitActivity> {
        let reference_day = calendar::transaction_date(reference, reference, zone);
        let mut activity = transactions
            .iter()
            .filter_map(|transaction| {
                let date = calendar::transaction_date(transaction.posted_date(), reference, zone)?;
                if reference_day.is_none_or(|day| date > day)
                    || !transaction.amount.is_finite()
                    || transaction.amount == 0.0
                {
                    return None;
                }
                self.classify(transaction, date)
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

    fn classify(&self, transaction: &Transaction, date: NaiveDate) -> Option<BenefitActivity> {
        let text = transaction.descriptors();
        let rule = self.rules.iter().find(|rule| {
            let excluded = rule.excludes.as_ref().is_some_and(|re| re.is_match(&text))
                || merchant_amount(&rule.data.exclude_merchant_amounts, transaction)
                || rule
                    .requires_before
                    .as_ref()
                    .is_some_and(|(re, before)| date < *before && !re.is_match(&text));
            !excluded
                && (rule.matches.is_match(&text)
                    || rule
                        .credit_matches
                        .as_ref()
                        .is_some_and(|re| re.is_match(&text))
                    || merchant_amount(&rule.data.merchant_amount_matches, transaction))
        })?;
        let identified_credit = rule.data.issuer_credits
            && transaction.benefit_confirmed != Some(false)
            && (transaction.benefit_confirmed == Some(true)
                || (rule
                    .credit_matches
                    .as_ref()
                    .is_some_and(|re| re.is_match(&text))
                    && !REFUND.is_match(&text)));
        let kind = match (identified_credit, transaction.amount > 0.0) {
            (true, true) => ActivityKind::Credit,
            (true, false) => ActivityKind::Reversal,
            (false, false) => ActivityKind::Purchase,
            (false, true) => ActivityKind::Unverified,
        };
        let in_effect = rule.data.effective_from.is_none_or(|from| date >= from)
            && (rule.data.effective_until.is_none_or(|until| date <= until) || identified_credit);
        in_effect.then(|| BenefitActivity {
            id: transaction.id.clone(),
            merchant: transaction.merchant.clone(),
            description: transaction.description.clone(),
            amount: transaction.amount,
            pending: transaction.pending,
            account_id: transaction.account_id.clone(),
            date,
            benefit_id: rule.data.id.clone(),
            benefit_name: rule.data.name.clone(),
            kind,
            paired_id: None,
            credit_window: None,
            alternate_credit_window: None,
            credit: None,
        })
    }

    /// The benefit a positive transaction is an identified credit for, if any.
    pub fn credit_identity(
        &self,
        transaction: &Transaction,
        zone: &dyn Zone,
    ) -> Option<(String, String)> {
        self.activity(&[transaction], transaction.posted_date(), zone)
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
        let matched = self.activity(transactions, snapshot, zone);
        self.tracker_for(&matched, reference, snapshot, zone)
    }

    /// `tracker` over activity already classified as of `snapshot`.
    pub fn tracker_for(
        &self,
        matched: &[BenefitActivity],
        reference: &str,
        snapshot: &str,
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
            .map(|rule| self.window(rule, matched, reference_instant, snapshot_instant, zone))
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
    fn window_credits<'a>(
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
        let (unused_low, unused_high) = credits.unused(cap);
        let remaining_range = tracked.then(|| AmountRange {
            low: from_minor(rule.usable(unused_low)),
            high: from_minor(rule.usable(unused_high)),
        });
        let unusable_remainder = (tracked && !credits.uncertain())
            .then(|| unused_low - rule.usable(unused_low))
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
        let renewal_check = renewal_credit.and_then(|credit| {
            NaiveDate::from_ymd_opt(credit.date.year() + 4, credit.date.month(), 1)
                .map(|month| month + Duration::days(credit.date.day() as i64 - 1))
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
            engaged: ours().any(|item| {
                item.kind == ActivityKind::Purchase
                    || (item.kind == ActivityKind::Credit && !item.pending)
            }),
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

    /// What became of every calendar window that ended on a month end from `first` through
    /// the day before `reference`, judged with all activity known as of `reference`.
    pub fn window_outcomes(
        &self,
        transactions: &[&Transaction],
        first: NaiveDate,
        reference: &str,
        zone: &dyn Zone,
    ) -> Vec<WindowOutcome> {
        let Some(reference_day) = calendar::transaction_date(reference, reference, zone) else {
            return Vec::new();
        };
        // The feed can only speak for windows that its history fully covers.
        let history_start = transactions
            .iter()
            .filter_map(|transaction| {
                calendar::transaction_date(transaction.posted_date(), reference, zone)
            })
            .min();
        let matched = self.activity(transactions, reference, zone);
        let mut outcomes = Vec::new();
        let mut month = calendar::add_months_first(first, 0);
        while month <= reference_day {
            let window_end = calendar::month_end(month.year(), month.month());
            if window_end < reference_day {
                for rule in &self.rules {
                    let in_effect = rule
                        .data
                        .effective_from
                        .is_none_or(|from| from <= window_end)
                        && rule
                            .data
                            .effective_until
                            .is_none_or(|until| until >= window_end);
                    let period = rule.period(window_end);
                    if in_effect && period.cadence.is_calendar() && period.end == window_end {
                        outcomes.push(self.outcome(
                            rule,
                            &period,
                            &matched,
                            history_start,
                            reference_day,
                        ));
                    }
                }
            }
            month = calendar::add_months_first(month, 1);
        }
        outcomes.sort_by(|left, right| {
            left.window_end
                .cmp(&right.window_end)
                .then_with(|| left.benefit_id.cmp(&right.benefit_id))
        });
        outcomes
    }

    fn outcome(
        &self,
        rule: &Rule,
        period: &Period,
        matched: &[BenefitActivity],
        history_start: Option<NaiveDate>,
        today: NaiveDate,
    ) -> WindowOutcome {
        let definition = &rule.data;
        let credits = Self::window_credits(rule, period, matched, today);
        let lag = CreditLag::observe(rule, matched);
        let posted_purchase = credits
            .activity
            .iter()
            .any(|item| item.kind == ActivityKind::Purchase && !item.pending);
        let (state, unused) = if definition.estimate_from_purchases {
            // Uber Cash is spent before the card is charged, so only a card charge is evidence.
            if posted_purchase {
                (OutcomeState::Settled, Some((0, 0)))
            } else {
                (OutcomeState::NotVisible, None)
            }
        } else if !definition.track_remaining || !definition.issuer_credits {
            (OutcomeState::NotTracked, None)
        } else if history_start.is_none_or(|start| start > period.start) {
            (OutcomeState::PartialHistory, None)
        } else if today <= period.end + Duration::days(lag.days)
            || credits.activity.iter().any(|item| item.awaits_credit())
        {
            (OutcomeState::Settling, None)
        } else {
            let (low, high) = credits.unused(minor_units(period.cap));
            (
                OutcomeState::Settled,
                Some((rule.usable(low), rule.usable(high))),
            )
        };
        let engaged = matched.iter().any(|item| {
            item.benefit_id == definition.id
                && (item.kind == ActivityKind::Purchase
                    || (item.kind == ActivityKind::Credit && !item.pending))
        });
        WindowOutcome {
            window_end: period.end,
            benefit_id: definition.id.clone(),
            state,
            unused: unused.map(|(low, _)| from_minor(low)),
            unused_high: unused.map(|(_, high)| from_minor(high)),
            engaged,
            verdict: Verdict::of(state, engaged),
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
