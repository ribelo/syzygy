#[macro_export]
macro_rules! combine {
    ($r1:expr $(,)?) => {
        $r1
    };
    ($r1:expr, $r2:expr $(,)?) => {
        $crate::reducer::Combined::new($r1, $r2)
    };
    ($r1:expr, $r2:expr, $($rest:expr),+ $(,)?) => {
        $crate::combine!($crate::reducer::Combined::new($r1, $r2), $($rest),+)
    };
}

#[macro_export]
macro_rules! dispatch {
    ($ctx:expr, $handler:expr) => {
        $handler.handle((), $ctx)
    };
    ($ctx:expr, $handler:expr, $payload:expr) => {
        $handler.handle($payload, $ctx)
    };
}
