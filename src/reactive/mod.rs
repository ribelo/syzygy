use downcast_rs::{Downcast, impl_downcast};

pub mod graph;
pub mod handler;
pub mod port_id;

pub trait BoxableValue: Downcast {}
impl_downcast!(BoxableValue);
impl<T: 'static> BoxableValue for T {}
