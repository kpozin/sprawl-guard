use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{CodeLines, LeafFiles};

/// Limits applied to source files and directory fan-out.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Maximum line counts per source file.
    pub max_lines_per_file: MaxLinesPerFileLimits,
    /// Maximum direct leaf source files per directory.
    pub max_leaf_files_per_directory: MaxLeafFilesPerDirectoryLimits,
}

/// Line-count limits applied to source files by classification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaxLinesPerFileLimits {
    /// Maximum non-test code lines per production source file.
    pub code_non_test: CodeLines,
    /// Maximum code lines per source file classified as test code.
    pub code: CodeLines,
}

/// Directory fan-out limits applied to leaf source files by classification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaxLeafFilesPerDirectoryLimits {
    /// Maximum direct leaf files classified as production source per directory.
    pub production: LeafFiles,
    /// Maximum direct leaf files classified as test code per directory.
    pub test: TestFanoutLimit,
}

/// Directory fan-out limit for files classified as test code.
///
/// Test files are commonly colocated with production files. A finite value
/// enforces a test-file fan-out ceiling, while `Unlimited` keeps tests from
/// consuming the production leaf-file budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestFanoutLimit {
    /// Enforce the contained direct leaf test-file count.
    Finite(LeafFiles),
    /// Do not enforce a direct leaf test-file count.
    Unlimited,
}

impl TestFanoutLimit {
    /// Returns whether test directory fan-out is unlimited.
    pub const fn is_unlimited(self) -> bool {
        matches!(self, Self::Unlimited)
    }
}

impl Serialize for TestFanoutLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Finite(value) => value.serialize(serializer),
            Self::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de> Deserialize<'de> for TestFanoutLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RawTestFanoutLimit::deserialize(deserializer)? {
            RawTestFanoutLimit::Finite(value) => Ok(Self::Finite(value)),
            RawTestFanoutLimit::Keyword(value) if value == "unlimited" => Ok(Self::Unlimited),
            RawTestFanoutLimit::Keyword(value) => Err(serde::de::Error::custom(format!(
                "unsupported test fan-out limit {value:?}; expected a positive integer or \"unlimited\"",
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawTestFanoutLimit {
    Finite(LeafFiles),
    Keyword(String),
}

pub(super) fn merge_limits(config: &mut Limits, partial: PartialLimits) {
    if let Some(value) = partial.max_lines_per_file {
        if let Some(value) = value.code_non_test {
            config.max_lines_per_file.code_non_test = value;
        }
        if let Some(value) = value.code {
            config.max_lines_per_file.code = value;
        }
    }
    if let Some(value) = partial.max_leaf_files_per_directory {
        if let Some(value) = value.production {
            config.max_leaf_files_per_directory.production = value;
        }
        if let Some(value) = value.test {
            config.max_leaf_files_per_directory.test = value;
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialLimits {
    max_lines_per_file: Option<PartialMaxLinesPerFileLimits>,
    max_leaf_files_per_directory: Option<PartialMaxLeafFilesPerDirectoryLimits>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialMaxLinesPerFileLimits {
    code_non_test: Option<CodeLines>,
    code: Option<CodeLines>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialMaxLeafFilesPerDirectoryLimits {
    production: Option<LeafFiles>,
    test: Option<TestFanoutLimit>,
}
