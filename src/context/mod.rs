use crate::model::Model;

#[cfg(feature = "async")]
pub mod r#async;

pub trait Context: Sized {
    type Model: Model;
}

// Removed FromContext and IntoContext traits in favor of standard From/Into traits
