//! UI responsiveness instrumentation (test utilities).
//!
//! Provides helpers for measuring callback durations to validate that
//! no callback exceeds 50 ms blocking (SC-005 requirement).  Uses tracing spans
//! with structured fields so measurements can be exported via `tracing-subscriber`.

#[cfg(test)]
mod tests {
    use std::{
        future::{Future, Ready, ready},
        time::{Duration, Instant},
    };

    use {
        tokio::{test as TokioTest, time::sleep},
        tracing::{Span, info_span, warn},
    };

    struct DurationGuard {
        name: &'static str,
        start: Instant,
    }

    impl DurationGuard {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                start: Instant::now(),
            }
        }
    }

    impl Drop for DurationGuard {
        fn drop(&mut self) {
            let elapsed_ms = u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX);
            warn_slow_sync(self.name, elapsed_ms);
        }
    }

    fn warn_slow_sync(name: &str, elapsed_ms: u64) {
        if elapsed_ms > 50 {
            warn!(
                name,
                duration_ms = elapsed_ms,
                "Synchronous callback exceeded 50 ms threshold",
            );
        }
    }

    fn fast_future() -> Ready<i32> {
        ready(42)
    }

    async fn slow_future() -> &'static str {
        sleep(Duration::from_millis(60)).await;
        "done"
    }

    fn warn_slow_async(name: &str, elapsed_ms: u64) {
        if elapsed_ms > 50 {
            warn!(
                name,
                duration_ms = elapsed_ms,
                "Main-loop callback exceeded 50 ms threshold",
            );
        }
    }

    async fn instrumented<F, T>(name: &'static str, future: F) -> T
    where
        F: Future<Output = T>,
    {
        let start = Instant::now();
        let span: Span = info_span!("callback", name);
        let guard = span.enter();
        let result = future.await;
        drop(guard);
        let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        warn_slow_async(name, elapsed_ms);
        result
    }

    #[TokioTest]
    async fn instrumented_under_threshold() {
        instrumented("fast_op", fast_future()).await;
    }

    #[TokioTest]
    async fn instrumented_over_threshold() {
        instrumented("slow_op", slow_future()).await;
    }

    #[test]
    fn duration_guard_drops_within_threshold() {
        drop(DurationGuard::new("ok_op"));
    }
}
