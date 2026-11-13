use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CountLines, DirectFileLimit, LineLimit};

/// Limits applied to source files and to direct files per directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Maximum line counts per source file.
    pub max_lines_per_file: MaxLinesPerFileLimits,
    /// Maximum direct source files per directory.
    pub max_direct_files_per_directory: MaxDirectFilesPerDirectoryLimits,
}

/// Line-count limits applied to source files by classification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaxLinesPerFileLimits {
    /// Line types counted in each file measurement.
    pub count_lines: CountLines,
    /// Maximum lines per production source file.
    pub production: LineLimit,
    /// Maximum lines per test source file.
    pub test: LineLimit,
}

/// Directory direct-file count limits applied to source files by classification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaxDirectFilesPerDirectoryLimits {
    /// Maximum direct production files, or unlimited measurement.
    pub production: DirectFileLimit,
    /// Maximum direct test files, or unlimited measurement.
    pub test: DirectFileLimit,
}

pub(super) fn merge_limits(config: &mut Limits, partial: PartialLimits) {
    if let Some(value) = partial.max_lines_per_file {
        if let Some(value) = value.count_lines {
            config.max_lines_per_file.count_lines = value;
        }
        if let Some(value) = value.production {
            config.max_lines_per_file.production = value;
        }
        if let Some(value) = value.test {
            config.max_lines_per_file.test = value;
        }
    }
    if let Some(value) = partial.max_direct_files_per_directory {
        merge_max_direct_files_per_directory_limits(
            &mut config.max_direct_files_per_directory,
            value,
        );
    }
}

pub(super) fn merge_max_direct_files_per_directory_limits(
    config: &mut MaxDirectFilesPerDirectoryLimits,
    partial: PartialMaxDirectFilesPerDirectoryLimits,
) {
    if let Some(value) = partial.production {
        config.production = value;
    }
    if let Some(value) = partial.test {
        config.test = value;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialLimits {
    max_lines_per_file: Option<PartialMaxLinesPerFileLimits>,
    max_direct_files_per_directory: Option<PartialMaxDirectFilesPerDirectoryLimits>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PartialMaxLinesPerFileLimits {
    count_lines: Option<CountLines>,
    production: Option<LineLimit>,
    test: Option<LineLimit>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialMaxDirectFilesPerDirectoryLimits {
    production: Option<DirectFileLimit>,
    test: Option<DirectFileLimit>,
}
