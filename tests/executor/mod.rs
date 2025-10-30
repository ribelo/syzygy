//! Executor test modules

#[cfg(feature = "tokio")]
mod tokio_executor_basic;

#[cfg(feature = "rt-single-thread")]
mod single_thread_executor_basic;
