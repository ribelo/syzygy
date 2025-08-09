use crate::model::Model;

#[cfg(feature = "async")]
pub mod snapshot;

pub trait Context: Sized {
    type Model: Model;
    type Event;
}

// Removed FromContext and IntoContext traits in favor of standard From/Into traits
