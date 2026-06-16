use std::sync::LazyLock;
use std::time::{Duration, Instant};

static PERF_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("AMM_PERF")
        .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
});

pub struct ScopeTimer {
    label: String,
    start: Option<Instant>,
    threshold_ms: u128,
}

impl ScopeTimer {
    pub fn with_threshold(label: impl Into<String>, threshold_ms: u128) -> Self {
        if !enabled() {
            return Self {
                label: String::new(),
                start: None,
                threshold_ms,
            };
        }

        Self {
            label: label.into(),
            start: Some(Instant::now()),
            threshold_ms,
        }
    }
}

impl Drop for ScopeTimer {
    fn drop(&mut self) {
        let Some(start) = self.start else {
            return;
        };
        log_elapsed_if_slow(&self.label, start.elapsed(), self.threshold_ms);
    }
}

pub fn enabled() -> bool {
    *PERF_ENABLED
}

pub fn now() -> Instant {
    Instant::now()
}

pub fn log_elapsed_with_threshold(label: impl AsRef<str>, start: Instant, threshold_ms: u128) {
    if !enabled() {
        return;
    }
    log_elapsed_if_slow(label.as_ref(), start.elapsed(), threshold_ms);
}

fn log_elapsed_if_slow(label: &str, elapsed: Duration, threshold_ms: u128) {
    if elapsed.as_millis() < threshold_ms {
        return;
    }
    tracing::info!(
        target: "perf",
        elapsed_ms = elapsed.as_secs_f64() * 1000.0,
        "{}",
        label
    );
}
