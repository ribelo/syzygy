use std::any::TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortId(TypeId);

impl PortId {
    #[must_use]
    pub fn new<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

pub trait PortTuple {
    fn port_ids() -> Vec<PortId>;
}

macro_rules! impl_port_tuple {
    ($($T:ident),*) => {
        impl<$($T),*> PortTuple for ($($T,)*)
        where
            $($T: 'static,)*
        {
            fn port_ids() -> Vec<PortId> {
                vec![$(PortId::new::<$T>(),)*]
            }
        }
    }
}

impl_port_tuple!(T1);
impl_port_tuple!(T1, T2);
impl_port_tuple!(T1, T2, T3);
impl_port_tuple!(T1, T2, T3, T4);
impl_port_tuple!(T1, T2, T3, T4, T5);
impl_port_tuple!(T1, T2, T3, T4, T5, T6);
impl_port_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_port_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_port_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_port_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_port_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_port_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
