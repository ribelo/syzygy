
#[cfg(feature = "async")]
pub mod r#async;
pub mod event;
pub mod thread;

pub trait Context: Sized + Clone  {
    fn execute<'a, H, T, R>(&'a self, h: H) -> R
        where
            H: ContextExecutor<'a, Self, T, R>,
        {
            h.call(self)
        }
}

pub trait FromContext<C>
where
    C: Context,
{
    fn from_context(cx: &C) -> Self;
}

pub trait BorrowFromContext<'a, C>
where
    C: Context,
{
    fn from_context(cx: &'a C) -> Self;
}

pub trait ContextExecutor<'a, C, T, R>
where
    C: Context,
{
    fn call(self, cx: &'a C) -> R;
}

impl<'a, C, F, R> ContextExecutor<'a, C, (), R> for F
where
    C: Context,
    F: FnOnce() -> R,
{
    fn call(self, _cx: &'a C) -> R {
        (self)()
    }
}

macro_rules! impl_context_executor {
    ($($t:ident),*) => {
        impl<'a, C, F, $($t,)* R> ContextExecutor<'a, C, ($($t,)*), R> for F
        where
            C: Context,
            F: FnOnce($($t,)*) -> R,
            $($t: BorrowFromContext<'a, C>,)*
        {
            fn call(self, cx: &'a C) -> R {
                (self)($($t::from_context(cx),)*)
            }
        }
    }
}

impl_context_executor!(T1);
impl_context_executor!(T1, T2);
impl_context_executor!(T1, T2, T3);
impl_context_executor!(T1, T2, T3, T4);
impl_context_executor!(T1, T2, T3, T4, T5);
impl_context_executor!(T1, T2, T3, T4, T5, T6);
impl_context_executor!(T1, T2, T3, T4, T5, T6, T7);
impl_context_executor!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_context_executor!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_context_executor!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_context_executor!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_context_executor!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

#[cfg(test)]
mod tests {
    use crate::model::{Model, ModelMut};
    use crate::resource::Resource;
    use crate::{context::Context, syzygy::SyzygyBuilder};

    #[test]
    fn test_context_executor() {
        pub struct TestModel {
            counter: i32,
        }
        #[derive(Clone)]
        pub struct TestResource {
            name: String,
        }

        let model = TestModel { counter: 0 };
        let resource = TestResource {
            name: "test".to_string(),
        };
        let syzygy = SyzygyBuilder::default()
            .model(model)
            .resource(resource)
            .build();

        syzygy.execute(|Model(model): Model<TestModel>| println!("counter is {}", model.counter));
        syzygy.execute(|ModelMut(mut model): ModelMut<TestModel>| model.counter += 1);
        syzygy.execute(|Model(model): Model<TestModel>| println!("counter is {}", model.counter));
        syzygy.execute(|Resource(resource): Resource<TestResource>| println!("name is {}", resource.name));

        syzygy.execute(|| println!("Hello, world!"));
    }
}
