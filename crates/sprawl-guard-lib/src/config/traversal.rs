use std::fmt;
use std::num::{NonZeroU32, NonZeroU64};

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
    /// Maximum byte size for a source file before line counting.
    pub max_file_bytes: SourceFileByteLimit,
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

/// Error returned when a source file byte limit is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SourceFileByteLimitError {
    /// Source file byte limit must allow non-empty files.
    #[error("traversal max_file_bytes must be greater than zero")]
    Zero,
}

/// Maximum source file size in bytes before line counting.
#[derive(Clone, Copy, Debug, Eq, Ord, JsonSchema, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct SourceFileByteLimit(NonZeroU64);

impl SourceFileByteLimit {
    /// Creates a source file byte limit.
    pub fn new(value: u64) -> Result<Self, SourceFileByteLimitError> {
        let Some(value) = NonZeroU64::new(value) else {
            return Err(SourceFileByteLimitError::Zero);
        };
        Ok(Self(value))
    }

    /// Creates a source file byte limit from a trusted built-in value.
    pub(crate) const fn trusted(value: u64) -> Self {
        let Some(value) = NonZeroU64::new(value) else {
            panic!("trusted source file byte limit must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw byte limit.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for SourceFileByteLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.get().fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for SourceFileByteLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
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
    if let Some(value) = partial.max_file_bytes {
        config.max_file_bytes = value;
    }
    if let Some(value) = partial.unreadable_paths {
        config.unreadable_paths = value;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialTraversalConfig {
    pub(super) follow_symlinks: Option<SymlinkTraversal>,
    pub(super) same_file_system: Option<bool>,
    pub(super) max_entries: Option<TraversalEntryLimit>,
    pub(super) max_file_bytes: Option<SourceFileByteLimit>,
    pub(super) unreadable_paths: Option<UnreadablePathPolicy>,
}
