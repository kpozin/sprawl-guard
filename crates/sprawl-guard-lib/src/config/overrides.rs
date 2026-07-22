use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use super::limits::TestFanoutLimit;
use crate::{CodeLines, GlobPattern, LeafFiles};

/// Error returned when an override reason is invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OverrideReasonError {
    /// Override reason was empty or whitespace only.
    #[error("override reason must not be empty")]
    Empty,
}

/// Human-readable override rationale.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct OverrideReason(String);

impl OverrideReason {
    /// Creates an override rationale.
    pub fn new(value: impl Into<String>) -> Result<Self, OverrideReasonError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(OverrideReasonError::Empty);
        }
        Ok(Self(value))
    }

    /// Returns the underlying reason string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for OverrideReason {
    type Error = OverrideReasonError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for OverrideReason {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Error returned when an override path list is invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OverridePathsError {
    /// Override path list was empty.
    #[error("override paths must not be empty")]
    Empty,
}

/// Non-empty list of glob patterns matched against normalized paths.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OverridePaths(Vec<GlobPattern>);

impl OverridePaths {
    /// Creates a non-empty override path list.
    pub fn new(paths: Vec<GlobPattern>) -> Result<Self, OverridePathsError> {
        if paths.is_empty() {
            return Err(OverridePathsError::Empty);
        }
        Ok(Self(paths))
    }

    /// Returns override path patterns as a slice.
    pub fn as_slice(&self) -> &[GlobPattern] {
        &self.0
    }
}

impl fmt::Display for OverridePaths {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let paths = self
            .0
            .iter()
            .map(GlobPattern::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        formatter.write_str(&paths)
    }
}

impl<'de> Deserialize<'de> for OverridePaths {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(Vec::<GlobPattern>::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Error returned when override limits are invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum OverrideLimitsError {
    /// Override had no limit fields.
    #[error("override must set at least one limit")]
    Empty,
}

/// Non-empty set of limits applied by a path override.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OverrideLimits {
    /// Optional file line-count limits.
    #[serde(skip_serializing_if = "OverrideMaxLinesPerFileLimits::is_empty")]
    max_lines_per_file: OverrideMaxLinesPerFileLimits,
    /// Optional directory fan-out limits.
    #[serde(skip_serializing_if = "OverrideMaxLeafFilesPerDirectoryLimits::is_empty")]
    max_leaf_files_per_directory: OverrideMaxLeafFilesPerDirectoryLimits,
}

/// Optional file line-count limits supplied by a path override.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OverrideMaxLinesPerFileLimits {
    /// Optional production file non-test code-line limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    code_non_test: Option<CodeLines>,
    /// Optional test-code file code-line limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<CodeLines>,
}

impl OverrideMaxLinesPerFileLimits {
    fn is_empty(&self) -> bool {
        self.code_non_test.is_none() && self.code.is_none()
    }
}

/// Optional directory fan-out limits supplied by a path override.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OverrideMaxLeafFilesPerDirectoryLimits {
    /// Optional production leaf-file fan-out limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    production: Option<LeafFiles>,
    /// Optional test-code leaf-file fan-out limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    test: Option<TestFanoutLimit>,
}

impl OverrideMaxLeafFilesPerDirectoryLimits {
    fn is_empty(&self) -> bool {
        self.production.is_none() && self.test.is_none()
    }
}

impl OverrideLimits {
    fn new(
        max_lines_per_file: OverrideMaxLinesPerFileLimits,
        max_leaf_files_per_directory: OverrideMaxLeafFilesPerDirectoryLimits,
    ) -> Result<Self, OverrideLimitsError> {
        if max_lines_per_file.is_empty() && max_leaf_files_per_directory.is_empty() {
            return Err(OverrideLimitsError::Empty);
        }
        Ok(Self {
            max_lines_per_file,
            max_leaf_files_per_directory,
        })
    }

    /// Returns the production file non-test code-line override, when present.
    pub fn code_non_test_lines_per_file(&self) -> Option<CodeLines> {
        self.max_lines_per_file.code_non_test
    }

    /// Returns the production directory fan-out override, when present.
    pub fn production_leaf_files_per_directory(&self) -> Option<LeafFiles> {
        self.max_leaf_files_per_directory.production
    }

    /// Returns the test-code file code-line override, when present.
    pub fn code_lines_per_test_file(&self) -> Option<CodeLines> {
        self.max_lines_per_file.code
    }

    /// Returns the test-code directory fan-out override, when present.
    pub fn test_leaf_files_per_directory(&self) -> Option<TestFanoutLimit> {
        self.max_leaf_files_per_directory.test
    }
}

impl<'de> Deserialize<'de> for OverrideLimits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let partial = PartialOverrideLimits::deserialize(deserializer)?;
        let max_lines_per_file = partial.max_lines_per_file.unwrap_or_default();
        let max_leaf_files_per_directory = partial.max_leaf_files_per_directory.unwrap_or_default();
        Self::new(
            max_lines_per_file.into(),
            max_leaf_files_per_directory.into(),
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialOverrideLimits {
    max_lines_per_file: Option<PartialOverrideMaxLinesPerFileLimits>,
    max_leaf_files_per_directory: Option<PartialOverrideMaxLeafFilesPerDirectoryLimits>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialOverrideMaxLinesPerFileLimits {
    code_non_test: Option<CodeLines>,
    code: Option<CodeLines>,
}

impl From<PartialOverrideMaxLinesPerFileLimits> for OverrideMaxLinesPerFileLimits {
    fn from(value: PartialOverrideMaxLinesPerFileLimits) -> Self {
        Self {
            code_non_test: value.code_non_test,
            code: value.code,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialOverrideMaxLeafFilesPerDirectoryLimits {
    production: Option<LeafFiles>,
    test: Option<TestFanoutLimit>,
}

impl From<PartialOverrideMaxLeafFilesPerDirectoryLimits>
    for OverrideMaxLeafFilesPerDirectoryLimits
{
    fn from(value: PartialOverrideMaxLeafFilesPerDirectoryLimits) -> Self {
        Self {
            production: value.production,
            test: value.test,
        }
    }
}

/// Path override entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PathOverride {
    /// Non-empty glob patterns matched against normalized paths.
    pub paths: OverridePaths,
    /// Non-empty limit set applied when `paths` match.
    #[serde(flatten)]
    pub limits: OverrideLimits,
    /// Optional human-readable rationale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<OverrideReason>,
}
