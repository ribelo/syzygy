use std::{any::TypeId, marker::PhantomData};

use super::{
    graph::{Graph, Node},
    port_id::PortId,
};

pub trait FromGraph: {
    fn from_graph(graph: &Graph) -> Self;
    fn deps_ids() -> Vec<PortId>;
}

// impl FromGraph for () {
//     fn from_graph(_graph: &Graph) -> Self {}
//     // fn deps_ids() -> Vec<PortId> {
//     //     vec![]
//     // }
// }

pub trait ReactiveHandler<T, R>
where
    T: 'static,
    R: 'static,
{
    fn call(&self, graph: &Graph) -> R;
    fn deps_ids(&self) -> Vec<PortId>;
}

impl<F, R> ReactiveHandler<(), R> for F
where
    F: Fn() -> R,
    R: 'static,
{
    fn call(&self, _graph: &Graph) -> R {
        (self)()
    }
    fn deps_ids(&self) -> Vec<PortId> {
        vec![]
    }
}

macro_rules! tuple_impls {
    ($($t:ident),*; $f:ident) => {
        impl<$($t),*, R, $f> ReactiveHandler<($($t,)*), R> for $f
        where
            $($t: FromGraph + 'static,)*
            $f: Fn($($t),*) -> R,
            R: 'static,
        {
            fn call(&self, graph: &Graph) -> R {
                (self)($(<$t>::from_graph(graph),)*)
            }
            fn deps_ids(&self) -> Vec<PortId> {
                let mut deps = Vec::new();
                $(deps.extend(<$t as FromGraph>::deps_ids());)*
                deps
            }
            // fn port_id() -> PortId {
            //     PortId::new::<R>()
            // }
        }
    }
}

macro_rules! impl_handler {
    (($($t:ident),*), $f:ident) => {
        tuple_impls!($($t),*; $f);
    };
}

impl_handler!((T1), F);
impl_handler!((T1, T2), F);
impl_handler!((T1, T2, T3), F);
impl_handler!((T1, T2, T3, T4), F);
impl_handler!((T1, T2, T3, T4, T5), F);
impl_handler!((T1, T2, T3, T4, T5, T6), F);
impl_handler!((T1, T2, T3, T4, T5, T6, T7), F);
impl_handler!((T1, T2, T3, T4, T5, T6, T7, T8), F);
impl_handler!((T1, T2, T3, T4, T5, T6, T7, T8, T9), F);
impl_handler!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10), F);
impl_handler!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11), F);
impl_handler!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12), F);

pub trait Reactive: {
    fn run(&self, graph: &Graph);
}

pub struct ReactiveWrapper<T, H, R>
where
    T: 'static,
    H: ReactiveHandler<T, R>,
    R: 'static,
{
    pub handler: H,
    pub phantom: PhantomData<(T, R)>,
}

impl<T, H, R> ReactiveWrapper<T, H, R>
where
    T: 'static,
    H: ReactiveHandler<T, R>,
    R: 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            phantom: PhantomData,
        }
    }
}

impl<T, H, R> Reactive for ReactiveWrapper<T, H, R>
where
    T:  'static,
    H: ReactiveHandler<T, R>  + 'static,
    R:  'static,
{
    fn run(&self, graph: &Graph) {
        let r = self.handler.call(graph);
        graph.set_node_value_by_id(PortId::new::<R>(), r);
    }
}

// impl<T, R> EffectHandlerFn<R> for T
// where
//     T: ReactiveHandler<T, R>,
//     R: Send + Sync,
// {
//     fn call(&self, graph: &Graph) {
//         let r = self.call(graph);
//     }
// }

// pub(crate) struct EffectHandlerWrapper<S, H, T, R>
// where
//     S: Send + Sync + Clone + 'static,
//     H: ReactiveHandler<S, T, R> + Copy,
// {
//     pub handler: H,
//     pub phantom: PhantomData<(S, T, R)>,
// }

// impl<S, H, T, R> Copy for EffectHandlerWrapper<S, H, T, R>
// where
//     S: Send + Sync + Clone + 'static,
//     H: ReactiveHandler<S, T, R> + Copy,
//     T: Send + Sync,
// {
// }

// impl<S, H, T, R> Clone for EffectHandlerWrapper<S, H, T, R>
// where
//     S: Send + Sync + Clone + 'static,
//     H: ReactiveHandler<S, T, R> + Copy,
//     T: Send + Sync,
// {
//     fn clone(&self) -> Self {
//         *self
//     }
// }

// #[async_trait]
// impl<S, H, T, R> EffectHandlerFn<S, R> for EffectHandlerWrapper<S, H, T, R>
// where
//     S: Send + Sync + Clone + 'static,
//     H: ReactiveHandler<S, T, R> + Copy,
//     T: Send + Sync,
//     R: Send + Sync,
// {
//     async fn call_with_context(&self, context: &mut EffectContext<S>) {
//         self.handler.call(context).await;
//     }
//     async fn collect_dependencies(&self, deps: &mut HashSet<Id>) {
//         self.handler.collect_dependencies(deps);
//     }
// }

// #[async_trait]
// impl<S> FromEffectContext<S> for Eve<S>
// where
//     S: Send + Sync + Clone + 'static,
// {
//     async fn from_context(context: &mut EffectContext<S>) -> Self {
//         context.eve.clone()
//     }
// }

// #[async_trait]
// impl<S, T: BoxableValue + Clone> FromEffectContext<S> for Node<T>
// where
//     S: Send + Sync + Clone + 'static,
// {
//     async fn from_context(context: &mut EffectContext<S>) -> Self {
//         Node(context.eve.get_node::<T>().await.unwrap())
//     }

//     fn collect_dependencies(deps: &mut HashSet<Id>) {
//         deps.insert(Id::new::<T>());
//     }
// }
