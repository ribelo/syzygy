use syzygy::prelude::*;

#[derive(Debug, Default, Clone, PartialEq, Model)]
struct AppModel {
    #[model(wrapper = Sum)]
    sum: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    NoArgs,
    One(i32),
    Two(i32, i32),
    Twelve(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {}

fn no_args(sum: &mut Sum) -> Command<AppEvent, AppEffect> {
    **sum = 0;
    Command::none()
}

fn one_arg(value: i32, sum: &mut Sum) -> Command<AppEvent, AppEffect> {
    **sum = value;
    Command::none()
}

fn two_args((left, right): (i32, i32), sum: &mut Sum) -> Command<AppEvent, AppEffect> {
    **sum = left + right;
    Command::none()
}

fn twelve_args(
    (a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12): (
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ),
    sum: &mut Sum,
) -> Command<AppEvent, AppEffect> {
    **sum = a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12;
    Command::none()
}

fn handle_event(event: AppEvent, ctx: &EventContext<AppModel>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::NoArgs => handle!(no_args, ctx),
        AppEvent::One(v) => handle!(one_arg, ctx, v),
        AppEvent::Two(a, b) => handle!(two_args, ctx, a, b),
        AppEvent::Twelve(a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12) => {
            handle!(
                twelve_args,
                ctx,
                a1,
                a2,
                a3,
                a4,
                a5,
                a6,
                a7,
                a8,
                a9,
                a10,
                a11,
                a12
            )
        }
    }
}

#[test]
fn handle_macro_supports_zero_and_one_payload_arg() {
    let mut store = TestStore::new(AppModel::default(), handle_event);

    store.send(AppEvent::NoArgs);
    assert_eq!(store.state().sum, 0);

    store.send(AppEvent::One(7));
    assert_eq!(store.state().sum, 7);
}

#[test]
fn handle_macro_wraps_two_payload_args_into_tuple() {
    let mut store = TestStore::new(AppModel::default(), handle_event);

    store.send(AppEvent::Two(3, 4));

    assert_eq!(store.state().sum, 7);
}

#[test]
fn handle_macro_supports_twelve_payload_args() {
    let mut store = TestStore::new(AppModel::default(), handle_event);

    store.send(AppEvent::Twelve(1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1));

    assert_eq!(store.state().sum, 12);
}
