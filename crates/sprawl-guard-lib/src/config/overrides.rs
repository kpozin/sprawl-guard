use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{DirectFileLimit, GlobPattern, LineLimit};

/// Error returned when an override reason is invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OverrideReasonError {
    /// Override reason was empty or whitespace only.
    #[error("override reason must not be empty")]
    Empty,
}

/// Human-readable override rationale.
#[derive(Clone, Debug, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct OverrideReason(#[schemars(pattern(r".*\S.*"))] String);

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
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OverridePaths(#[schemars(length(min = 1))] Vec<GlobPattern>);

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
    /// Optional directory direct-file limits.
    #[serde(skip_serializing_if = "OverrideMaxDirectFilesPerDirectoryLimits::is_empty")]
    max_direct_files_per_directory: OverrideMaxDirectFilesPerDirectoryLimits,
}

/// Optional file line-count limits supplied by a path override.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OverrideMaxLinesPerFileLimits {
    /// Optional production line limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    production: Option<LineLimit>,
    /// Optional test line limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    test: Option<LineLimit>,
}

impl OverrideMaxLinesPerFileLimits {
    fn is_empty(&self) -> bool {
        self.production.is_none() && self.test.is_none()
    }
}

/// Optional directory direct-file limits supplied by a path override.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OverrideMaxDirectFilesPerDirectoryLimits {
    /// Optional production direct-file limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    production: Option<DirectFileLimit>,
    /// Optional test-code direct-file limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    test: Option<DirectFileLimit>,
}

impl OverrideMaxDirectFilesPerDirectoryLimits {
    fn is_empty(&self) -> bool {
        self.production.is_none() && self.test.is_none()
    }
}

impl OverrideLimits {
    fn new(
        max_lines_per_file: OverrideMaxLinesPerFileLimits,
        max_direct_files_per_directory: OverrideMaxDirectFilesPerDirectoryLimits,
    ) -> Result<Self, OverrideLimitsError> {
        if max_lines_per_file.is_empty() && max_direct_files_per_directory.is_empty() {
            return Err(OverrideLimitsError::Empty);
        }
        Ok(Self {
            max_lines_per_file,
            max_direct_files_per_directory,
        })
    }

    /// Returns the production line override, when present.
    pub fn production_loc_per_file(&self) -> Option<LineLimit> {
        self.max_lines_per_file.production
    }

    /// Returns the production directory direct-file override, when present.
    pub fn production_direct_files_per_directory(&self) -> Option<DirectFileLimit> {
        self.max_direct_files_per_directory.production
    }

    /// Returns the test line override, when present.
    pub fn test_loc_per_file(&self) -> Option<LineLimit> {
        self.max_lines_per_file.test
    }

    /// Returns the test-code directory direct-file override, when present.
    pub fn test_direct_files_per_directory(&self) -> Option<DirectFileLimit> {
        self.max_direct_files_per_directory.test
    }
}

impl<'de> Deserialize<'de> for OverrideLimits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let partial = PartialOverrideLimits::deserialize(deserializer)?;
        let max_lines_per_file = partial.max_lines_per_file.unwrap_or_default();
        let max_direct_files_per_directory =
            partial.max_direct_files_per_directory.unwrap_or_default();
        Self::new(
            max_lines_per_file.into(),
            max_direct_files_per_directory.into(),
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PartialOverrideLimits {
    max_lines_per_file: Option<PartialOverrideMaxLinesPerFileLimits>,
    max_direct_files_per_directory: Option<PartialOverrideMaxDirectFilesPerDirectoryLimits>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("minProperties" = 1))]
struct PartialOverrideMaxLinesPerFileLimits {
    production: Option<LineLimit>,
    test: Option<LineLimit>,
}

impl From<PartialOverrideMaxLinesPerFileLimits> for OverrideMaxLinesPerFileLimits {
    fn from(value: PartialOverrideMaxLinesPerFileLimits) -> Self {
        Self {
            production: value.production,
            test: value.test,
        }
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("minProperties" = 1))]
struct PartialOverrideMaxDirectFilesPerDirectoryLimits {
    production: Option<DirectFileLimit>,
    test: Option<DirectFileLimit>,
}

impl From<PartialOverrideMaxDirectFilesPerDirectoryLimits>
    for OverrideMaxDirectFilesPerDirectoryLimits
{
    fn from(value: PartialOverrideMaxDirectFilesPerDirectoryLimits) -> Self {
        Self {
            production: value.production,
            test: value.test,
        }
    }
}

/// Path override entry.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
// The schema requires at least one limit. Each `anyOf` branch also declares the property it
// requires, because strict validators such as SchemaStore's reject a schema that lists a property
// in `required` without declaring it in that schema's `properties`.
#[schemars(extend(
    "anyOf" = [
        { "properties": { "max_lines_per_file": {} }, "required": ["max_lines_per_file"] },
        {
            "properties": { "max_direct_files_per_directory": {} },
            "required": ["max_direct_files_per_directory"]
        }
    ]
))]
pub struct PathOverride {
    /// Non-empty glob patterns matched against normalized paths.
    pub paths: OverridePaths,
    /// Non-empty limit set applied when `paths` match.
    #[serde(flatten)]
    #[schemars(with = "PartialOverrideLimits")]
    pub limits: OverrideLimits,
    /// Human-readable rationale.
    pub reason: OverrideReason,
}
