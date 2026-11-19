use std::fmt;
use std::num::NonZeroU32;

use nutype::nutype;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

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
    /// How unreadable paths below the root affect `check`.
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

/// How the scan handles descendant paths it cannot read.
///
/// This does not affect failures to resolve the root or selected paths, or read and count failures for
/// already-discovered source files. Those errors make the check result
/// incomplete and remain operational failures.
#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnreadablePathPolicy {
    /// List the unreadable paths found while scanning, then fail.
    Fail,
    /// Report the unreadable paths found while scanning as warnings, and continue.
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

/// Key of the parsed-code file byte limit in `[traversal]`.
pub(super) const BYTE_LIMIT_KEY: &str = "max_parsed_code_file_bytes";

/// Former name of [`BYTE_LIMIT_KEY`], still accepted on load.
pub(super) const DEPRECATED_BYTE_LIMIT_KEY: &str = "max_file_bytes";

/// Lists the deprecated byte-limit name in the schema, so schema validation accepts
/// what the loader accepts and editors mark the old name as deprecated.
///
/// The serde alias alone is invisible to schemars.
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
        property.insert("deprecated".to_owned(), true.into());
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
    if let Some(value) = partial.max_parsed_code_file_bytes {
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
    pub(super) max_parsed_code_file_bytes: Option<ParsedCodeFileByteLimit>,
    pub(super) unreadable_paths: Option<UnreadablePathPolicy>,
}
