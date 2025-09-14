//! Executor test modules

#[cfg(feature = "tokio")]
mod tokio_executor_tests;

#[cfg(feature = "tokio")]
mod io_runtime_tests;

mod single_thread_executor_tests;



mod runtime_registration;
