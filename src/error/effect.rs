use thiserror::Error;

/// Errors that can occur in effect handlers
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EffectError {
    /// Generic error represented as string message
    #[error("Effect error: {0}")]
    Message(String),
}

impl From<String> for EffectError {
    fn from(s: String) -> Self {
        EffectError::Message(s)
    }
}

impl From<&str> for EffectError {
    fn from(s: &str) -> Self {
        EffectError::Message(s.to_string())
    }
}
