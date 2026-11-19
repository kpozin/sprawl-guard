use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU32;

use nutype::nutype;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};

use crate::value::byte_size::{byte_count_schemas, deserialize_byte_count};

/// Filesystem traversal settings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TraversalConfig {
    /// Which symlinks may be followed during source discovery.
    pub follow_symlinks: SymlinkTraversal,
    /// Whether native traversal stays on the root's filesystem.
    ///
    /// WASI accepts and reports this configured value but cannot enforce it.
    pub same_file_system: bool,
    /// Maximum number of filesystem entries yielded during source discovery.
    pub max_entries: TraversalEntryLimit,
    /// Largest source file, in bytes, whose code lines are parsed and counted.
    pub max_parsed_code_file_bytes: ParsedCodeFileByteLimit,
    /// Whether gaps in analysis fail `check`.
    pub unreadable_paths: UnreadablePathPolicy,
}

/// Which symlinks source discovery follows.
#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymlinkTraversal {
    /// Do not follow symlinks.
    None,
    /// Follow symlinks that resolve to regular files.
    Files,
    /// Follow symlinked directories during traversal.
    Directories,
    /// Follow both symlinked files and symlinked directories.
    FilesAndDirectories,
}

impl SymlinkTraversal {
    /// Returns whether symlinked files count as source files.
    pub const fn follows_files(self) -> bool {
        matches!(self, Self::Files | Self::FilesAndDirectories)
    }

    /// Returns whether discovery follows symlinked directories.
    pub const fn follows_directories(self) -> bool {
        matches!(self, Self::Directories | Self::FilesAndDirectories)
    }
}

/// Whether gaps in analysis fail the check.
///
/// Gaps are paths below the root that could not be read, including discovered source files, and
/// source files too large to count. The check reports every gap and every violation under either
/// policy. Failures to resolve the root or selected paths still stop the run.
#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnreadablePathPolicy {
    /// Report every gap, and fail the check with exit code `2` when there are any.
    Fail,
    /// Report every gap as a warning, and let violations alone decide the result.
    Warn,
}

/// Error returned when a traversal entry limit is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TraversalEntryLimitError {
    /// Traversal entry limit must allow at least one filesystem entry.
    #[error("traversal max_entries must be greater than zero")]
    Zero,
}

/// Maximum number of filesystem entries source discovery may inspect.
#[derive(Clone, Copy, Debug, Eq, Ord, JsonSchema, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct TraversalEntryLimit(NonZeroU32);

impl TraversalEntryLimit {
    /// Creates a traversal entry limit.
    pub fn new(value: u32) -> Result<Self, TraversalEntryLimitError> {
        let Some(value) = NonZeroU32::new(value) else {
            return Err(TraversalEntryLimitError::Zero);
        };
        Ok(Self(value))
    }

    /// Creates a traversal entry limit from a trusted built-in value.
    pub(crate) const fn trusted(value: u32) -> Self {
        let Some(value) = NonZeroU32::new(value) else {
            panic!("trusted traversal entry limit must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw entry limit.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for TraversalEntryLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.get().fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for TraversalEntryLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_u32_limit(deserializer, Self::new)
    }
}

fn deserialize_u32_limit<'de, D, T, E>(
    deserializer: D,
    constructor: impl FnOnce(u32) -> std::result::Result<T, E>,
) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    E: std::fmt::Display,
{
    constructor(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

/// Error returned when a parsed-code file byte limit is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ParsedCodeFileByteLimitError {
    /// The limit must allow non-empty files.
    #[error("traversal max_parsed_code_file_bytes must be greater than zero")]
    Zero,
}

const fn validate_parsed_code_file_byte_limit(
    value: &u64,
) -> Result<(), ParsedCodeFileByteLimitError> {
    if *value == 0 {
        Err(ParsedCodeFileByteLimitError::Zero)
    } else {
        Ok(())
    }
}

/// Largest file, in bytes, whose code lines are parsed and counted.
#[nutype(
    const_fn,
    validate(with = validate_parsed_code_file_byte_limit, error = ParsedCodeFileByteLimitError),
    derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Display, Serialize, Deserialize),
    derive_unchecked(JsonSchema),
)]
pub struct ParsedCodeFileByteLimit(#[schemars(range(min = 1))] u64);

/// `max_parsed_code_file_bytes` as configuration writes it: a positive number of bytes or a size
/// string.
#[derive(Debug)]
pub(super) struct ConfiguredParsedCodeFileByteLimit(ParsedCodeFileByteLimit);

impl<'de> Deserialize<'de> for ConfiguredParsedCodeFileByteLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        ParsedCodeFileByteLimit::try_new(deserialize_byte_count(deserializer)?)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for ConfiguredParsedCodeFileByteLimit {
    fn schema_name() -> Cow<'static, str> {
        "ParsedCodeFileByteLimit".into()
    }

    /// Distinct from the ID of [`ParsedCodeFileByteLimit`]'s own schema, so a schema holding both
    /// keeps two definitions rather than letting one replace the other.
    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::ConfiguredParsedCodeFileByteLimit").into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        let [count, size] = byte_count_schemas();
        json_schema!({
            "description": "Largest file whose code lines are parsed and counted: a positive number of bytes or a size such as \"10 MiB\".",
            "anyOf": [count, size]
        })
    }
}

/// Key of the parsed-code file byte limit in `[traversal]`.
pub(super) const BYTE_LIMIT_KEY: &str = "max_parsed_code_file_bytes";

/// Former name of [`BYTE_LIMIT_KEY`], still accepted on load.
pub(super) const DEPRECATED_BYTE_LIMIT_KEY: &str = "max_file_bytes";

/// Lists the deprecated byte-limit name in the schema, so schema validation accepts
/// what the loader accepts and editors describe the old name as deprecated.
///
/// The serde alias alone is invisible to schemars. The description carries the deprecation
/// because the published schema is draft-04, which has no `deprecated` keyword, and SchemaStore
/// rejects unknown keywords.
fn add_deprecated_byte_limit_name(schema: &mut schemars::Schema) {
    let Some(properties) = schema
        .get_mut("properties")
        .and_then(|properties| properties.as_object_mut())
    else {
        return;
    };
    let Some(mut deprecated) = properties.get(BYTE_LIMIT_KEY).cloned() else {
        return;
    };
    if let Some(property) = deprecated.as_object_mut() {
        property.insert(
            "description".to_owned(),
            "Deprecated name of `max_parsed_code_file_bytes`. Run `sprawl-guard config upgrade` to rename it."
                .into(),
        );
    }
    properties.insert(DEPRECATED_BYTE_LIMIT_KEY.to_owned(), deprecated);
}

pub(super) fn merge_traversal(config: &mut TraversalConfig, partial: PartialTraversalConfig) {
    if let Some(value) = partial.follow_symlinks {
        config.follow_symlinks = value;
    }
    if let Some(value) = partial.same_file_system {
        config.same_file_system = value;
    }
    if let Some(value) = partial.max_entries {
        config.max_entries = value;
    }
    if let Some(ConfiguredParsedCodeFileByteLimit(value)) = partial.max_parsed_code_file_bytes {
        config.max_parsed_code_file_bytes = value;
    }
    if let Some(value) = partial.unreadable_paths {
        config.unreadable_paths = value;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = add_deprecated_byte_limit_name)]
pub(super) struct PartialTraversalConfig {
    pub(super) follow_symlinks: Option<SymlinkTraversal>,
    pub(super) same_file_system: Option<bool>,
    pub(super) max_entries: Option<TraversalEntryLimit>,
    /// Largest source file, in bytes, whose code lines are parsed and counted.
    #[serde(alias = "max_file_bytes")]
    pub(super) max_parsed_code_file_bytes: Option<ConfiguredParsedCodeFileByteLimit>,
    pub(super) unreadable_paths: Option<UnreadablePathPolicy>,
}
