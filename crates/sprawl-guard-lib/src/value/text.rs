use std::fmt;

use serde::Serialize;

/// Human-readable backend or parser detail included in diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DiagnosticDetail(String);

impl DiagnosticDetail {
    /// Creates diagnostic detail text.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the detail text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns whether the detail text is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for DiagnosticDetail {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
