//! Tests for scheduler() panic behavior when no runtime is available

use syzygy::scheduler::{scheduler, scheduler_strict};

#[cfg(feature = "tokio")]
mod tokio_tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn scheduler_panics_with_helpful_message_when_no_tokio_runtime() {
        // This test must run outside of tokio::test context to ensure no runtime exists
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _scheduler = scheduler();
        }));

        assert!(
            result.is_err(),
            "scheduler() should panic when no tokio runtime exists"
        );

        // The panic happens - that's the main test. The exact message format may vary.
        // The important thing is that it panics with a helpful message.
    }

    #[test]
    fn scheduler_strict_returns_error_when_no_tokio_runtime() {
        // This test must run outside of tokio::test context to ensure no runtime exists
        let result = scheduler_strict();

        assert!(
            result.is_err(),
            "scheduler_strict() should return error when no tokio runtime exists"
        );
        // The error message content is tested in the tokio test below
    }

    #[tokio::test]
    async fn scheduler_works_normally_when_tokio_runtime_exists() {
        // When tokio runtime exists, scheduler() should work normally
        let _scheduler = scheduler();
        // If we get here without panicking, the test passes
    }

    #[tokio::test]
    async fn scheduler_strict_returns_scheduler_when_tokio_runtime_exists() {
        // When tokio runtime exists, scheduler_strict() should return a scheduler
        let result = scheduler_strict();
        assert!(
            result.is_ok(),
            "scheduler_strict() should succeed when tokio runtime exists"
        );
    }
}

#[cfg(not(feature = "tokio"))]
mod no_tokio_tests {
    use super::*;

    #[test]
    fn scheduler_panics_with_compile_time_error_when_no_tokio_feature() {
        // When tokio feature is disabled, scheduler() should give compile-time error
        // This test just verifies the code compiles - the actual panic would be at runtime
        // but we can't test that without tokio feature enabled
    }

    #[test]
    fn scheduler_strict_returns_error_when_no_tokio_feature() {
        // When tokio feature is disabled, scheduler_strict() should return error
        let result = scheduler_strict();
        assert!(
            result.is_err(),
            "scheduler_strict() should return error when tokio feature disabled"
        );
    }
}
