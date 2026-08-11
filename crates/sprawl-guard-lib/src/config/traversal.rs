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
    /// Whether native traversal should stay on the root's filesystem.
    ///
    /// WASI accepts and reports this configured value but cannot enforce it.
    pub same_file_system: bool,
    /// Maximum traversal depth from the repository root.
    pub max_depth: TraversalDepth,
    /// Maximum number of filesystem entries yielded during source discovery.
    pub max_entries: TraversalEntryLimit,
    /// Maximum byte size for a source file before configured-LOC counting.
    pub max_file_bytes: SourceFileByteLimit,
    /// How recoverable descendant traversal errors affect `check`.
    pub error_policy: TraversalErrorPolicy,
}

/// Symlink following policy for source discovery.
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
    /// Returns whether symlinked files should be counted as source files.
    pub const fn follows_files(self) -> bool {
        matches!(self, Self::Files | Self::FilesAndDirectories)
    }

    /// Returns whether traversal should descend through symlinked directories.
    pub const fn follows_directories(self) -> bool {
        matches!(self, Self::Directories | Self::FilesAndDirectories)
    }
}

/// Policy for recoverable errors found while walking descendant paths.
///
/// This does not affect explicit root/scope failures or read/count failures for
/// already-discovered source files. Those errors make the check result
/// incomplete and remain operational failures.
#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TraversalErrorPolicy {
    /// Collect recoverable traversal incidents, then fail the run.
    Fail,
    /// Report recoverable traversal incidents as warnings and continue.
    Warn,
}

/// Error returned when a traversal depth limit is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TraversalDepthError {
    /// Traversal depth must include at least one path level.
    #[error("traversal max_depth must be greater than zero")]
    Zero,
}

/// Maximum filesystem traversal depth from the repository root.
#[derive(Clone, Copy, Debug, Eq, Ord, JsonSchema, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct TraversalDepth(NonZeroU32);

impl TraversalDepth {
    /// Creates a traversal depth limit.
    pub fn new(value: u32) -> Result<Self, TraversalDepthError> {
        let Some(value) = NonZeroU32::new(value) else {
            return Err(TraversalDepthError::Zero);
        };
        Ok(Self(value))
    }

    /// Creates a traversal depth limit from a trusted built-in value.
    pub(crate) const fn trusted(value: u32) -> Self {
        let Some(value) = NonZeroU32::new(value) else {
            panic!("trusted traversal depth must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw depth as a `usize` for traversal backends.
    pub const fn get(self) -> usize {
        self.0.get() as usize
    }
}

impl fmt::Display for TraversalDepth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.get().fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for TraversalDepth {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_u32_limit(deserializer, Self::new)
    }
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

/// Maximum source file size in bytes before configured-LOC counting.
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
    if let Some(value) = partial.max_depth {
        config.max_depth = value;
    }
    if let Some(value) = partial.max_entries {
        config.max_entries = value;
    }
    if let Some(value) = partial.max_file_bytes {
        config.max_file_bytes = value;
    }
    if let Some(value) = partial.error_policy {
        config.error_policy = value;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialTraversalConfig {
    follow_symlinks: Option<SymlinkTraversal>,
    same_file_system: Option<bool>,
    max_depth: Option<TraversalDepth>,
    max_entries: Option<TraversalEntryLimit>,
    max_file_bytes: Option<SourceFileByteLimit>,
    error_policy: Option<TraversalErrorPolicy>,
}
