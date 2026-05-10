#[macro_export]
macro_rules! handle {
    ($handler:expr, $ctx:expr $(,)?) => {
        $ctx.handle($handler)
    };
    ($handler:expr, $ctx:expr, $arg1:expr $(,)?) => {
        $handler.handle($arg1, $ctx)
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr $(,)?) => {
        $handler.handle(($arg1, $arg2), $ctx)
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr $(,)?) => {
        $handler.handle(($arg1, $arg2, $arg3), $ctx)
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr $(,)?) => {
        $handler.handle(($arg1, $arg2, $arg3, $arg4), $ctx)
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr $(,)?) => {
        $handler.handle(($arg1, $arg2, $arg3, $arg4, $arg5), $ctx)
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr $(,)?) => {
        $handler.handle(($arg1, $arg2, $arg3, $arg4, $arg5, $arg6), $ctx)
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr, $arg7:expr $(,)?) => {
        $handler.handle(($arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7), $ctx)
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr, $arg7:expr, $arg8:expr $(,)?) => {
        $handler.handle(
            ($arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7, $arg8),
            $ctx,
        )
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr, $arg7:expr, $arg8:expr, $arg9:expr $(,)?) => {
        $handler.handle(
            (
                $arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7, $arg8, $arg9,
            ),
            $ctx,
        )
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr, $arg7:expr, $arg8:expr, $arg9:expr, $arg10:expr $(,)?) => {
        $handler.handle(
            (
                $arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7, $arg8, $arg9, $arg10,
            ),
            $ctx,
        )
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr, $arg7:expr, $arg8:expr, $arg9:expr, $arg10:expr, $arg11:expr $(,)?) => {
        $handler.handle(
            (
                $arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7, $arg8, $arg9, $arg10, $arg11,
            ),
            $ctx,
        )
    };
    ($handler:expr, $ctx:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr, $arg7:expr, $arg8:expr, $arg9:expr, $arg10:expr, $arg11:expr, $arg12:expr $(,)?) => {
        $handler.handle(
            (
                $arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7, $arg8, $arg9, $arg10, $arg11,
                $arg12,
            ),
            $ctx,
        )
    };
}
