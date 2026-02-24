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
macro_rules! scope {
    ($child:expr, |$state:ident| $state_lens:expr, $event_variant:path, $effect_variant:path $(,)?) => {
        $crate::reducer::Reducer::scope(
            $child,
            |$state| $state_lens,
            |event| match event {
                $event_variant(child_event) => Some(child_event),
                _ => None,
            },
            $event_variant,
            $effect_variant,
        )
    };
}

#[macro_export]
macro_rules! for_each {
    ($child:expr, |$state:ident| $state_lens:expr, $id_extractor:expr, $event_from:expr, $event_into:expr, $effect_into:expr $(,)?) => {
        $crate::reducer::ReducerExt::for_each(
            $child,
            |$state| $state_lens,
            $id_extractor,
            $event_from,
            $event_into,
            $effect_into,
        )
    };
}

#[macro_export]
macro_rules! if_let {
    ($child:expr, |$state:ident| $state_lens:expr, $event_variant:path, $effect_variant:path $(,)?) => {
        $crate::if_let::if_let(
            $child,
            |$state| $state_lens,
            |event| match event {
                $event_variant(child_event) => child_event,
                _ => unreachable!("event variant did not match if_let! routing"),
            },
            $event_variant,
            $effect_variant,
        )
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
