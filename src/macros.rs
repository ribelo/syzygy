#[macro_export]
macro_rules! handle {
    ($handler:expr, $ctx:expr) => {
        $handler.handle((), $ctx)
    };
    ($handler:expr, $payload:expr, $ctx:expr) => {
        $handler.handle($payload, $ctx)
    };
}
