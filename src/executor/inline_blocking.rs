use super::{BlockingExecutor, ExecutorError, ExecutorLifecycle};

pub struct InlineBlocking;

impl Default for InlineBlocking {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineBlocking {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Debug for InlineBlocking {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineBlocking").finish()
    }
}

impl BlockingExecutor for InlineBlocking {
    fn spawn_blocking(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), ExecutorError> {
        job();
        Ok(())
    }
}

impl ExecutorLifecycle for InlineBlocking {
    fn shutdown(&self) {}

    fn wait(&self) {}
}
