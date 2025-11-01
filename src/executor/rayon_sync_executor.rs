use crate::executor::{BlockingExecutor, ExecutorError, ExecutorLifecycle};
use rayon::ThreadPoolBuilder;
use std::marker::PhantomData;

pub struct RayonExecutor<E, R = ()>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    pool: rayon::ThreadPool,
    resources: R,
    _marker: PhantomData<E>,
}

pub struct RayonExecutorBuilder<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    threads: Option<usize>,
    thread_name_prefix: Option<String>,
    resources: R,
    _marker: PhantomData<E>,
}

const DEFAULT_THREAD_PREFIX: &str = "syzygy-rayon";

impl<E> RayonExecutor<E, ()>
where
    E: Send + 'static,
{
    #[must_use]
    pub fn builder() -> RayonExecutorBuilder<E, ()> {
        RayonExecutorBuilder::new(())
    }
}

impl<E, R> RayonExecutor<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    #[must_use]
    pub fn resources(&self) -> &R {
        &self.resources
    }
}

impl<E, R> Default for RayonExecutor<E, R>
where
    E: Send + 'static,
    R: Default + Send + Sync + 'static,
{
    fn default() -> Self {
        RayonExecutor::builder().resources(R::default()).build()
    }
}

impl<E, R> BlockingExecutor for RayonExecutor<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    fn spawn_blocking(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), ExecutorError> {
        self.pool.spawn(job);
        Ok(())
    }
}

impl<E, R> ExecutorLifecycle for RayonExecutor<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    fn shutdown(&self) {}

    fn wait(&self) {
        // rayon pool does not need explicit waiting; it drains outstanding jobs
    }
}

impl<E, R> RayonExecutorBuilder<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    fn new(resources: R) -> Self {
        Self {
            threads: None,
            thread_name_prefix: None,
            resources,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads.max(1));
        self
    }

    #[must_use]
    pub fn thread_name_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.thread_name_prefix = Some(prefix.into());
        self
    }

    #[must_use]
    pub fn resources<New>(self, resources: New) -> RayonExecutorBuilder<E, New>
    where
        New: Send + Sync + 'static,
    {
        let Self {
            threads,
            thread_name_prefix,
            resources: _,
            _marker: marker,
        } = self;

        RayonExecutorBuilder {
            threads,
            thread_name_prefix,
            resources,
            _marker: marker,
        }
    }

    pub fn build(self) -> RayonExecutor<E, R> {
        let threads = resolve_threads(self.threads);
        let prefix = self
            .thread_name_prefix
            .unwrap_or_else(|| DEFAULT_THREAD_PREFIX.to_string());
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(move |i| format!("{prefix}-{i}"))
            .build()
            .expect("failed to build rayon pool");

        RayonExecutor {
            pool,
            resources: self.resources,
            _marker: PhantomData,
        }
    }
}

fn resolve_threads(explicit: Option<usize>) -> usize {
    explicit
        .filter(|&n| n > 0)
        .unwrap_or_else(default_thread_count)
}

fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
}
