//! Numeric helpers shared by the engine, insights, and providers: money in whole cents and
//! robust statistics. Each rule lives here once so every module rounds the same way.

/// Whole cents from a dollar amount, halves away from zero, as the engine's decimal math
/// rounds them.
pub fn minor_units(value: f64) -> i64 {
    (value * 100.0).round() as i64
}

/// How one return compares with another, in percent: within 0.05 points reads as `inline`,
/// else `ahead` or `behind`. Account returns and today's portfolio move both read it.
pub fn compare_returns(you: f64, benchmark: f64) -> &'static str {
    if (you - benchmark).abs() < 0.05 {
        "inline"
    } else if you > benchmark {
        "ahead"
    } else {
        "behind"
    }
}

/// A figure to two decimal places (dollars to whole cents), rounded as `minor_units` rounds.
pub fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Decimal dollars to whole cents, halves away from zero like `minor_units`.
pub fn cents(value: rust_decimal::Decimal) -> rust_decimal::Decimal {
    value.round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero)
}

/// Dollars from whole cents.
pub fn from_minor(minor: i64) -> f64 {
    minor as f64 / 100.0
}

/// The median of `values`, averaging the middle pair of an even count. Sorts in place and is
/// `None` for no values.
pub fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

/// The integer median of `values`, with the middle pair of an even count averaged by integer
/// division. Sorts in place and is `None` for no values.
pub fn median_i64(values: &mut [i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2
    } else {
        values[middle]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_round_halves_away_from_zero() {
        // 1.005 is a little under its half in binary.
        assert_eq!(minor_units(1.005), 100);
        assert_eq!(minor_units(2.675), 268);
        assert_eq!(minor_units(-0.005), -1);
        assert_eq!(minor_units(0.125), 13);
        assert_eq!(minor_units(-1.5), -150);
        assert_eq!(from_minor(-150), -1.5);
    }

    #[test]
    fn medians_average_the_middle_pair_and_reject_nothing() {
        assert_eq!(median(&mut []), None);
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median_i64(&mut []), None);
        assert_eq!(median_i64(&mut [4, 1, 3, 2]), Some(2));
        assert_eq!(median_i64(&mut [5, 1, 3]), Some(3));
    }
}
