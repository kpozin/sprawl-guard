use serde::{Deserialize, Serialize};

use crate::{IncludedLineTypes, LeafFileLimit, LocLimit};

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
    /// Logical line categories included in each file measurement.
    pub included_line_types: IncludedLineTypes,
    /// Maximum configured LOC per production source file.
    pub production: LocLimit,
    /// Maximum configured LOC per test source file.
    pub test: LocLimit,
}

/// Directory fan-out limits applied to leaf source files by classification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaxLeafFilesPerDirectoryLimits {
    /// Maximum direct production leaf files, or unlimited measurement.
    pub production: LeafFileLimit,
    /// Maximum direct test leaf files, or unlimited measurement.
    pub test: LeafFileLimit,
}

pub(super) fn merge_limits(config: &mut Limits, partial: PartialLimits) {
    if let Some(value) = partial.max_lines_per_file {
        if let Some(value) = value.included_line_types {
            config.max_lines_per_file.included_line_types = value;
        }
        if let Some(value) = value.production {
            config.max_lines_per_file.production = value;
        }
        if let Some(value) = value.test {
            config.max_lines_per_file.test = value;
        }
    }
    if let Some(value) = partial.max_leaf_files_per_directory {
        merge_max_leaf_files_per_directory_limits(&mut config.max_leaf_files_per_directory, value);
    }
}

pub(super) fn merge_max_leaf_files_per_directory_limits(
    config: &mut MaxLeafFilesPerDirectoryLimits,
    partial: PartialMaxLeafFilesPerDirectoryLimits,
) {
    if let Some(value) = partial.production {
        config.production = value;
    }
    if let Some(value) = partial.test {
        config.test = value;
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
    included_line_types: Option<IncludedLineTypes>,
    production: Option<LocLimit>,
    test: Option<LocLimit>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialMaxLeafFilesPerDirectoryLimits {
    production: Option<LeafFileLimit>,
    test: Option<LeafFileLimit>,
}
