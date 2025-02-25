use downcast_rs::{impl_downcast, Downcast};
use std::fmt::Debug;

pub mod handler;
pub mod graph;
pub mod port_id;


pub trait BoxableValue: Downcast {}
impl_downcast!(BoxableValue);
impl<T: 'static> BoxableValue for T {}
