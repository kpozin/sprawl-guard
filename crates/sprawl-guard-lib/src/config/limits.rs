use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};

use crate::value::byte_size::{
    ConfiguredByteLimit, UNLIMITED, byte_count_schemas, deserialize_byte_limit,
};
use crate::value::file_bytes::FileByteLimit;
use crate::value::limits::FiniteOrUnlimited;
use crate::{CountLines, DirectFileLimit, FileStorage, LineLimit, MaxBytesPerFile};

/// Limits applied to source files, to direct files per directory, and to the size of every file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Maximum line counts per source file.
    pub max_lines_per_file: MaxLinesPerFileLimits,
    /// Maximum direct source files per directory.
    pub max_direct_files_per_directory: MaxDirectFilesPerDirectoryLimits,
    /// Repository-wide maximum size of any file that is not in Git LFS, source or not, before path
    /// overrides.
    pub max_bytes_per_file: MaxBytesPerFile,
    /// Repository-wide maximum size of the content of any file that Git LFS stores, before path
    /// overrides.
    pub max_bytes_per_lfs_object: MaxBytesPerFile,
}

impl Limits {
    /// Returns the repository-wide byte limit for files stored in `storage`.
    pub const fn max_file_bytes(&self, storage: FileStorage) -> MaxBytesPerFile {
        match storage {
            FileStorage::Regular => self.max_bytes_per_file,
            FileStorage::Lfs => self.max_bytes_per_lfs_object,
        }
    }
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
    if let Some(ConfiguredMaxBytesPerFile(value)) = partial.max_bytes_per_file {
        config.max_bytes_per_file = value;
    }
    if let Some(ConfiguredMaxBytesPerFile(value)) = partial.max_bytes_per_lfs_object {
        config.max_bytes_per_lfs_object = value;
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
    max_bytes_per_file: Option<ConfiguredMaxBytesPerFile>,
    max_bytes_per_lfs_object: Option<ConfiguredMaxBytesPerFile>,
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

/// `max_bytes_per_file` as configuration writes it: a number of bytes, a size with a unit, or
/// `"unlimited"`.
#[derive(Clone, Copy, Debug)]
pub(super) struct ConfiguredMaxBytesPerFile(pub(super) MaxBytesPerFile);

impl<'de> Deserialize<'de> for ConfiguredMaxBytesPerFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let limit = match deserialize_byte_limit(deserializer)? {
            ConfiguredByteLimit::Bytes(bytes) => FiniteOrUnlimited::Finite(
                FileByteLimit::try_new(bytes).map_err(serde::de::Error::custom)?,
            ),
            ConfiguredByteLimit::Unlimited => FiniteOrUnlimited::Unlimited,
        };
        Ok(Self(limit))
    }
}

impl JsonSchema for ConfiguredMaxBytesPerFile {
    fn schema_name() -> Cow<'static, str> {
        "MaxBytesPerFile".into()
    }

    /// Distinct from the ID of [`MaxBytesPerFile`]'s own schema, so a schema holding both keeps
    /// two definitions rather than letting one replace the other.
    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::ConfiguredMaxBytesPerFile").into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        let [count, size] = byte_count_schemas();
        json_schema!({
            "description": "Byte limit: a positive number of bytes, a size such as \"10 MB\" or \"1.5 GiB\", or \"unlimited\".",
            "anyOf": [count, size, { "type": "string", "const": UNLIMITED }]
        })
    }
}
