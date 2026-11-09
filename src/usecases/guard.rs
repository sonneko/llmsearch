//! クォータとレート制限の適用。

use std::sync::Arc;

use crate::core::config::quota::{
    self, Counters, ExceedInfo, Metric, QuotaDecision, QuotaPolicy, Usage,
};
use crate::core::errors::{AppError, Diagnostic, WarningCode};
use crate::ports::state::CounterStore;
use crate::ports::{Clock, Reporter};

/// クォータの消費と判定。
pub struct Guard {
    policy: QuotaPolicy,
    store: Arc<dyn CounterStore>,
    clock: Arc<dyn Clock>,
    reporter: Arc<dyn Reporter>,
    session: Usage,
    counters: Counters,
}

impl Guard {
    pub fn new(
        policy: QuotaPolicy,
        store: Arc<dyn CounterStore>,
        clock: Arc<dyn Clock>,
        reporter: Arc<dyn Reporter>,
    ) -> Result<Self, AppError> {
        let mut counters = store.load()?;
        let today = clock.today_local().format("%Y-%m-%d").to_string();
        if counters.day_key != today {
            counters.day = Usage::default();
            counters.day_key = today;
        }
        let now = clock.now_utc();
        counters
            .window
            .retain(|w| quota::window_active(w, now) || w.used == Usage::default());
        if !policy.enabled {
            reporter.verbose("クォータは無効（全ての上限を適用しない）");
        }
        Ok(Self {
            policy,
            store,
            clock,
            reporter,
            session: Usage::default(),
            counters,
        })
    }

    /// 消費前の確認。
    pub fn check(&self, metric: Metric, amount: u64) -> Result<(), AppError> {
        match quota::check(
            &self.policy,
            &self.counters,
            &self.session,
            metric,
            amount,
            self.clock.today_local(),
            self.clock.now_utc(),
        ) {
            QuotaDecision::Allowed { warn } => {
                if warn {
                    self.reporter.warn(&Diagnostic::new(
                        WarningCode::W5005,
                        format!("クォータ {} の上限に達している（継続中）", metric.as_str()),
                    ));
                }
                Ok(())
            }
            QuotaDecision::Exceeded(info) => Err(report_exceeded(&info)),
        }
    }

    /// 消費を確定する。
    pub fn consume(&mut self, metric: Metric, amount: u64) -> Result<(), AppError> {
        self.check(metric, amount)?;
        self.apply(metric, amount);
        self.persist()
    }

    /// 複数項目をまとめて消費する。
    pub fn consume_all(&mut self, usage: Usage) -> Result<(), AppError> {
        for metric in Metric::all() {
            let amount = usage.get(metric);
            if amount > 0 {
                self.check(metric, amount)?;
            }
        }
        for metric in Metric::all() {
            let amount = usage.get(metric);
            if amount > 0 {
                self.apply(metric, amount);
            }
        }
        self.persist()
    }

    fn apply(&mut self, metric: Metric, amount: u64) {
        self.session.add(metric, amount);
        self.counters.day.add(metric, amount);
        self.counters.total.add(metric, amount);
        let now = self.clock.now_utc();
        self.counters.updated_at = now.to_rfc3339();
        let minutes_list: Vec<u64> = self.policy.window.iter().map(|w| w.minutes).collect();
        for minutes in minutes_list {
            let index = self
                .counters
                .window
                .iter()
                .position(|w| w.minutes == minutes);
            match index {
                Some(i) => {
                    let active = quota::window_active(&self.counters.window[i], now);
                    if !active {
                        self.counters.window[i].started_at = now.to_rfc3339();
                        self.counters.window[i].used = Usage::default();
                    }
                    self.counters.window[i].used.add(metric, amount);
                }
                None => self.counters.window.push(quota::WindowCounter {
                    minutes,
                    started_at: now.to_rfc3339(),
                    used: Usage::of(metric, amount),
                }),
            }
        }
    }

    fn persist(&self) -> Result<(), AppError> {
        self.store.save(&self.counters)
    }

    /// 残量の一覧。
    pub fn metrics(&self) -> Vec<quota::MetricUsage> {
        quota::remaining(
            &self.policy,
            &self.counters,
            &self.session,
            self.clock.now_utc(),
            self.clock.today_local(),
        )
    }

    /// 1 項目の残量（表示用 `13/50 (session)`）。
    pub fn usage_line(&self, metric: Metric) -> String {
        let name = metric.as_str();
        for m in self.metrics() {
            if m.name == format!("session.{name}") {
                return format!("{}/{} (session)", m.used, m.limit.unwrap_or(0));
            }
        }
        String::new()
    }

    /// セッション内の消費量。
    pub fn session_usage(&self) -> Usage {
        self.session
    }

    /// ポリシー。
    pub fn policy(&self) -> &QuotaPolicy {
        &self.policy
    }

    /// カウンタ（表示用）。
    pub fn counters(&self) -> &Counters {
        &self.counters
    }
}

fn report_exceeded(info: &ExceedInfo) -> AppError {
    info.to_error()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::quota::Limits;
    use crate::ports::system::SystemClock;

    struct Silent;
    impl Reporter for Silent {
        fn info(&self, _m: &str) {}
        fn warn(&self, _d: &Diagnostic) {}
        fn verbose(&self, _m: &str) {}
    }

    struct MemoryCounters(std::sync::Mutex<Counters>);
    impl CounterStore for MemoryCounters {
        fn load(&self) -> Result<Counters, AppError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, counters: &Counters) -> Result<(), AppError> {
            *self.0.lock().unwrap() = counters.clone();
            Ok(())
        }
        fn reset(&self, _scope: &str) -> Result<(), AppError> {
            *self.0.lock().unwrap() = Counters::default();
            Ok(())
        }
    }

    fn guard_with(policy: QuotaPolicy) -> Guard {
        let store = Arc::new(MemoryCounters(std::sync::Mutex::new(Counters::default())));
        Guard::new(
            policy,
            store,
            Arc::new(SystemClock::new()),
            Arc::new(Silent),
        )
        .unwrap()
    }

    #[test]
    fn consumes_and_reports() {
        let mut guard = guard_with(QuotaPolicy::default());
        guard.consume(Metric::Searches, 1).unwrap();
        assert_eq!(guard.session_usage().searches, 1);
        assert!(guard.usage_line(Metric::Searches).contains("/50"));
    }

    #[test]
    fn blocks_when_limit_reached() {
        let policy = QuotaPolicy {
            session: Limits {
                searches: Some(1),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut guard = guard_with(policy);
        guard.consume(Metric::Searches, 1).unwrap();
        let err = guard.consume(Metric::Searches, 1).unwrap_err();
        assert_eq!(err.code, crate::core::errors::ErrorCode::E5005);
    }

    #[test]
    fn day_counter_resets_on_new_day() {
        let guard = guard_with(QuotaPolicy::default());
        assert_eq!(
            guard.counters().day_key,
            chrono::Local::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string()
        );
    }
}
