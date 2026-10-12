//! Alpha Vantage's daily request budget and cooldowns, and the earnings calendar it pays for.
use super::*;

/// Alpha Vantage allows 25 requests a day; only earnings use it now.
pub(super) const DAILY_REQUESTS: usize = 25;

impl NewsCache {
    pub(super) fn budget(&self, now: i64) -> Result<(usize, Option<String>), String> {
        let used: usize = self
            .0
            .query_row(
                "SELECT count(*) FROM requests WHERE at > ?1",
                [now - DAY],
                |r| r.get(0),
            )
            .map_err(|_| "Request budget is unavailable")?;
        let cooldown: Option<(i64, String)> = self
            .0
            .query_row("SELECT until, period FROM cooldown WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
            .map_err(|_| "Request budget is unavailable")?;
        Ok((
            DAILY_REQUESTS.saturating_sub(used),
            cooldown
                .filter(|(until, _)| *until > now)
                .map(|(_, period)| period),
        ))
    }

    // Persist before sending: failed requests count too, including across restarts.
    pub fn reserve(&mut self, now: i64) -> Result<(), String> {
        let (remaining, cooldown) = self.budget(now)?;
        if let Some(period) = cooldown {
            return Err(match period.as_str() {
                "daily" | "daily-confirmed" => {
                    "Alpha Vantage daily limit reached; try again tomorrow"
                }
                "minute" | "daily-probe" => "Rate-limited; try again in a minute",
                _ => "Rate-limited; try again later",
            }
            .into());
        }
        if remaining == 0 {
            return Err("Daily request budget used; try again later".into());
        }
        let tx = self
            .0
            .transaction()
            .map_err(|_| "Could not save request budget")?;
        tx.execute("DELETE FROM requests WHERE at <= ?1", [now - DAY])
            .map_err(|_| "Could not save request budget")?;
        tx.execute("INSERT INTO requests VALUES (?1)", [now])
            .map_err(|_| "Could not save request budget")?;
        tx.commit()
            .map_err(|_| "Could not save request budget".into())
    }

    pub fn provider_limit(&mut self, payload: &Value, now: i64) -> Result<(), String> {
        let message = payload
            .get("Note")
            .or_else(|| payload.get("Information"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        if message.contains("rate limit")
            || message.contains("call frequency")
            || message.contains("requests per day")
        {
            let daily = message.contains("per day") || message.contains("daily");
            // A probe belongs to one rate-limit incident. An old cooldown row
            // must not turn every later daily report into a full-day pause.
            let recently_probed: bool = self
                .0
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM cooldown WHERE period='daily-probe' AND until >= ?1 - ?2)",
                    params![now, MINUTE],
                    |r| r.get(0),
                )
                .map_err(|_| "Could not read provider quota status")?;
            let first_daily_report = daily && !recently_probed && self.budget(now)?.0 > 0;
            self.save_cooldown(
                if daily && !first_daily_report {
                    DAY
                } else {
                    MINUTE
                },
                if first_daily_report {
                    "daily-probe"
                } else if daily {
                    "daily-confirmed"
                } else {
                    "minute"
                },
                now,
            )?;
        }
        Ok(())
    }

    pub fn provider_throttle(&mut self, retry_after: Option<i64>, now: i64) -> Result<(), String> {
        let delay = retry_after.unwrap_or(MINUTE).clamp(1, DAY);
        let period = if delay >= DAY {
            "daily-confirmed"
        } else if delay <= MINUTE {
            "minute"
        } else {
            "temporary"
        };
        self.save_cooldown(delay, period, now)
    }

    pub(super) fn save_cooldown(
        &mut self,
        delay: i64,
        period: &str,
        now: i64,
    ) -> Result<(), String> {
        self.0
            .execute(
                "INSERT OR REPLACE INTO cooldown (id, until, period) VALUES (1, ?1, ?2)",
                params![now + delay, period],
            )
            .map_err(|_| "Could not save provider quota status")?;
        Ok(())
    }

    pub fn earnings(&self, now: i64) -> Result<Option<String>, String> {
        self.0
            .query_row(
                "SELECT body FROM earnings WHERE id=1 AND saved <= ?1 AND saved > ?2",
                params![now, now - 6 * 3600],
                |r| r.get(0),
            )
            .optional()
            .map(|body: Option<String>| {
                body.filter(|value| {
                    value.len() <= crate::providers::http::MAX_PROVIDER_RESPONSE_BYTES
                })
            })
            .map_err(|_| "Earnings cache is unavailable".into())
    }

    pub fn save_earnings(&self, body: &str, now: i64) -> Result<(), String> {
        if body.len() > crate::providers::http::MAX_PROVIDER_RESPONSE_BYTES {
            return Err("Earnings response is too large to save".into());
        }
        self.0
            .execute(
                "INSERT OR REPLACE INTO earnings VALUES (1, ?1, ?2)",
                params![body, now],
            )
            .map_err(|_| "Could not save earnings")?;
        Ok(())
    }
}
