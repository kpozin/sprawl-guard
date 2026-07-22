use std::fmt;

use globset::Glob;
use serde::{Deserialize, Deserializer, Serialize};

/// Error returned when a glob pattern value is invalid.
#[derive(Debug, thiserror::Error)]
pub enum GlobPatternError {
    /// Glob pattern was empty.
    #[error("glob pattern must not be empty")]
    Empty,
    /// Glob pattern could not be parsed by the glob engine.
    #[error("invalid glob pattern {pattern:?}: {source}")]
    Invalid {
        /// Invalid glob pattern.
        pattern: String,
        /// Glob parser error.
        source: globset::Error,
    },
}

/// Gitignore-style glob pattern.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct GlobPattern(String);

impl GlobPattern {
    /// Creates a glob pattern.
    pub fn new(value: impl Into<String>) -> Result<Self, GlobPatternError> {
        let value = value.into();
        if value.is_empty() {
            return Err(GlobPatternError::Empty);
        }
        Glob::new(&value).map_err(|source| GlobPatternError::Invalid {
            pattern: value.clone(),
            source,
        })?;
        Ok(Self(value))
    }

    /// Returns the underlying pattern string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GlobPattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl TryFrom<&str> for GlobPattern {
    type Error = GlobPatternError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for GlobPattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
