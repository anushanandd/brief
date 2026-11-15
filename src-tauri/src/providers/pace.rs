//! A pace for each provider whose published limit Brief's own requests could pass in a burst:
//! SEC EDGAR allows ten requests a second, Alpaca two hundred a minute. Every request to either
//! takes a token first; when none is left it waits for the next, so callers never see a 429
//! from Brief's own traffic. Alpha Vantage's daily budget is kept in the news cache instead.
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub(crate) struct Pace {
    /// Tokens added each second, and the most kept for a burst.
    rate: f64,
    burst: f64,
    state: tokio::sync::Mutex<(f64, Instant)>,
}

/// Eight a second with a burst of two, so no second ever holds more than EDGAR's ten.
pub(crate) static SEC: LazyLock<Pace> = LazyLock::new(|| Pace::new(8.0, 2.0));
/// Three a second with a burst of eight, so no minute holds more than 188 of Alpaca's 200 and
/// the market stream's own requests have room.
pub(crate) static ALPACA: LazyLock<Pace> = LazyLock::new(|| Pace::new(3.0, 8.0));

impl Pace {
    fn new(rate: f64, burst: f64) -> Self {
        Self {
            rate,
            burst,
            state: tokio::sync::Mutex::new((burst, Instant::now())),
        }
    }

    /// Waits until a request may be sent. The lock is held while waiting, so waiters go in turn.
    pub(crate) async fn take(&self) {
        let mut state = self.state.lock().await;
        let now = Instant::now();
        let (tokens, wait) = admit(
            state.0,
            now.duration_since(state.1).as_secs_f64(),
            self.rate,
            self.burst,
        );
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        *state = (tokens, now + wait);
    }
}

/// The tokens left after one request, and how long it waits: `tokens` refilled over `elapsed`
/// seconds at `rate`, capped at `burst`; with less than one, the wait for the rest of one.
fn admit(tokens: f64, elapsed: f64, rate: f64, burst: f64) -> (f64, Duration) {
    let available = (tokens + elapsed * rate).min(burst);
    if available >= 1.0 {
        (available - 1.0, Duration::ZERO)
    } else {
        (0.0, Duration::from_secs_f64((1.0 - available) / rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_passes_then_requests_wait_for_the_rate() {
        // Ten a second: ten go at once, the eleventh waits a tenth of a second.
        let mut tokens = 10.0;
        for _ in 0..10 {
            let (left, wait) = admit(tokens, 0.0, 10.0, 10.0);
            assert!(wait.is_zero());
            tokens = left;
        }
        let (left, wait) = admit(tokens, 0.0, 10.0, 10.0);
        assert_eq!((left, wait), (0.0, Duration::from_millis(100)));
        // Idle time refills only up to the burst.
        assert_eq!(admit(0.0, 1_000.0, 3.0, 30.0).0, 29.0);
    }

    /// Requests sent back to back for `seconds` from a full burst: the most any window holds.
    fn sent_within(rate: f64, burst: f64, seconds: f64) -> usize {
        let (mut tokens, mut clock, mut sent) = (burst, 0.0, 0);
        loop {
            let (left, wait) = admit(tokens, 0.0, rate, burst);
            clock += wait.as_secs_f64();
            if clock >= seconds {
                return sent;
            }
            tokens = left;
            sent += 1;
        }
    }

    #[test]
    fn no_window_passes_a_published_limit() {
        assert!(sent_within(SEC.rate, SEC.burst, 1.0) <= 10);
        assert!(sent_within(ALPACA.rate, ALPACA.burst, 60.0) <= 200);
    }
}
