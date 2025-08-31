use core::fmt;

/// Errors that can occur in effect handlers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectError {
    /// Generic error represented as string message
    Message(String),
}

impl fmt::Display for EffectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EffectError::Message(m) => write!(f, "Effect error: {}", m),
        }
    }
}

impl std::error::Error for EffectError {}

impl From<&str> for EffectError {
    fn from(s: &str) -> Self {
        EffectError::Message(s.to_string())
    }
}

impl From<String> for EffectError {
    fn from(s: String) -> Self {
        EffectError::Message(s)
    }
}

