use std::fmt;
use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize};

/// Error returned when a configured limit value is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LimitValueError {
    /// A configured limit was zero, but policy limits must be positive.
    #[error("{kind} must be greater than zero")]
    Zero {
        /// Limit kind that received the invalid value.
        kind: LimitValueKind,
    },
}

/// Kind of configured limit value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitValueKind {
    /// Maximum count of code lines.
    CodeLines,
    /// Maximum count of direct leaf files.
    LeafFiles,
}

impl fmt::Display for LimitValueKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CodeLines => formatter.write_str("code-line count"),
            Self::LeafFiles => formatter.write_str("leaf-file count"),
        }
    }
}

/// Count of code lines.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct CodeLines(NonZeroU32);

impl CodeLines {
    /// Creates a code-line count.
    pub fn new(value: u32) -> Result<Self, LimitValueError> {
        let Some(value) = NonZeroU32::new(value) else {
            return Err(LimitValueError::Zero {
                kind: LimitValueKind::CodeLines,
            });
        };
        Ok(Self(value))
    }

    /// Creates a code-line count from a trusted built-in value.
    pub(crate) const fn trusted(value: u32) -> Self {
        let Some(value) = NonZeroU32::new(value) else {
            panic!("trusted code-line count must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw count.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for CodeLines {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Count of direct leaf files.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct LeafFiles(NonZeroU32);

impl LeafFiles {
    /// Creates a leaf-file count.
    pub fn new(value: u32) -> Result<Self, LimitValueError> {
        let Some(value) = NonZeroU32::new(value) else {
            return Err(LimitValueError::Zero {
                kind: LimitValueKind::LeafFiles,
            });
        };
        Ok(Self(value))
    }

    /// Creates a leaf-file count from a trusted built-in value.
    pub(crate) const fn trusted(value: u32) -> Self {
        let Some(value) = NonZeroU32::new(value) else {
            panic!("trusted leaf-file count must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw count.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for LeafFiles {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for CodeLines {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for LeafFiles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Measured count of code lines; zero is valid for empty or comment-only files.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct CountedCodeLines(u32);

impl CountedCodeLines {
    /// Creates a measured code-line count.
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw measured count.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for CountedCodeLines {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
