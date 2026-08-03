//! Error and result types for the library.

use std::path::PathBuf;

use crate::classification::TraversalIncident;
use crate::config::LanguageGroupsError;
use crate::{LanguageGroupId, LanguageId, RelativePath};

/// Error type for recoverable `sprawl-guard` library failures.
#[derive(Debug, thiserror::Error)]
pub enum SprawlError {
    /// Language-group configuration was invalid.
    #[error(transparent)]
    LanguageGroups(#[from] LanguageGroupsError),
    /// The configured root path could not be resolved.
    #[error("failed to resolve root {path}: {source}")]
    ResolveRoot {
        /// Root path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A relative path was expected.
    #[error("path must be relative: {path}")]
    AbsolutePath {
        /// Invalid path.
        path: PathBuf,
    },
    /// A path used parent-directory traversal.
    #[error("path must not contain '..': {path}")]
    ParentPath {
        /// Invalid path.
        path: PathBuf,
    },
    /// A path was expected under a root.
    #[error("path {path} is not under root {root}")]
    PathOutsideRoot {
        /// Path that was outside the root.
        path: PathBuf,
        /// Expected root.
        root: PathBuf,
    },
    /// A requested check scope does not exist.
    #[error("check scope does not exist: {path}")]
    MissingCheckScope {
        /// Missing scope path as supplied by the caller.
        path: PathBuf,
    },
    /// An explicit config file does not exist.
    #[error("config file does not exist: {path}")]
    MissingConfig {
        /// Missing config path.
        path: PathBuf,
    },
    /// Both supported root config names exist.
    #[error("ambiguous config files: {primary} and {alias} both exist")]
    AmbiguousConfig {
        /// Primary config path.
        primary: PathBuf,
        /// Alias config path.
        alias: PathBuf,
    },
    /// A config file could not be read.
    #[error("failed to read {path}: {source}")]
    ReadConfig {
        /// Path that could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A directory could not be read.
    #[error("failed to read directory {path}: {source}")]
    ReadDirectory {
        /// Directory that could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A directory entry could not be read.
    #[error("failed to read entry under {path}: {source}")]
    ReadDirectoryEntry {
        /// Directory being enumerated.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A file type could not be read.
    #[error("failed to read file type for {path}: {source}")]
    ReadFileType {
        /// Path whose file type could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A source file could not be read for LOC counting.
    #[error("failed to read source {path}: {source}")]
    ReadSource {
        /// Source file path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A ratchet file could not be read.
    #[error("failed to read ratchet file {path}: {source}")]
    ReadRatchet {
        /// Ratchet file path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A ratchet file could not be written.
    #[error("failed to write ratchet file {path}: {source}")]
    WriteRatchet {
        /// Ratchet file path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// TOML could not be parsed.
    #[error("failed to parse {path}: {source}")]
    ParseToml {
        /// TOML file path.
        path: PathBuf,
        /// TOML parser error.
        source: toml::de::Error,
    },
    /// A ratchet TOML file could not be parsed.
    #[error("failed to parse ratchet file {path}: {source}")]
    ParseRatchet {
        /// Ratchet file path.
        path: PathBuf,
        /// TOML parser error.
        source: toml::de::Error,
    },
    /// A ratchet file used an unsupported schema version.
    #[error("unsupported ratchet file version {version}; expected version 1")]
    UnsupportedRatchetVersion {
        /// Version read from the ratchet file.
        version: u32,
    },
    /// A ratchet file contained duplicate entries for one metric section.
    #[error("duplicate ratchet entry in [[{section}]] for path {path}")]
    DuplicateRatchetEntry {
        /// Ratchet section containing the duplicate.
        section: &'static str,
        /// Duplicate ratchet path.
        path: RelativePath,
    },
    /// A ratchet file contained a duplicate grouped directory entry.
    #[error("duplicate directory ratchet entry in [[{section}]] for group {group} and path {path}")]
    DuplicateDirectoryRatchetEntry {
        /// Ratchet section containing the duplicate.
        section: &'static str,
        /// Duplicate language group.
        group: LanguageGroupId,
        /// Duplicate directory path.
        path: RelativePath,
    },
    /// An included config fragment was not a TOML table.
    #[error("included config {path} must be a TOML table")]
    IncludeFragmentNotTable {
        /// Included config path.
        path: PathBuf,
    },
    /// An included config fragment had an unsupported table.
    #[error("included config {path} may contain only [[overrides]], found [{table}]")]
    IncludeFragmentUnsupportedTable {
        /// Included config path.
        path: PathBuf,
        /// Unsupported table name.
        table: String,
    },
    /// A config referenced an unknown language.
    #[error("unknown language: {language}")]
    UnknownLanguage {
        /// Unknown language ID.
        language: String,
    },
    /// A config used a language-specific setting with an unsupported language.
    #[error("setting {setting} is not supported for language {language}")]
    LanguageSpecificSetting {
        /// Language where the setting appeared.
        language: String,
        /// Unsupported setting.
        setting: &'static str,
    },
    /// An enabled language had neither bundled nor authored included paths.
    #[error(
        "enabled language {language} must use bundled paths, a nonempty include, or nonempty additional_globs"
    )]
    LanguageWithoutIncludedPaths {
        /// Language whose authored path selection is empty.
        language: LanguageId,
    },
    /// A glob pattern is invalid.
    #[error("invalid config include glob {pattern:?}: {source}")]
    InvalidGlob {
        /// Glob pattern.
        pattern: String,
        /// Glob parser error.
        source: globset::Error,
    },
    /// Ignore-aware traversal failed.
    #[error("failed to traverse {path}: {source}")]
    Traverse {
        /// Path being traversed.
        path: PathBuf,
        /// Underlying traversal error.
        source: ignore::Error,
    },
    /// Ignore-aware traversal collected recoverable incidents.
    #[error(
        "traversal failed under {root}: {}",
        format_traversal_incident_count(incidents)
    )]
    TraversalFailed {
        /// Traversed repository root.
        root: PathBuf,
        /// Collected traversal incidents.
        incidents: Vec<TraversalIncident>,
    },
    /// Source discovery inspected more filesystem entries than allowed.
    #[error("traversal entry limit exceeded under {root}: saw more than {limit} entries")]
    TraversalEntryLimitExceeded {
        /// Traversed repository root.
        root: PathBuf,
        /// Configured entry limit.
        limit: u32,
    },
    /// A source file exceeded the configured byte-size limit.
    #[error("source {path} is too large to count: {size} bytes exceeds limit {limit}")]
    SourceFileTooLarge {
        /// Source file path.
        path: PathBuf,
        /// File size in bytes.
        size: u64,
        /// Configured byte-size limit.
        limit: u64,
    },
    /// A path matched more than one enabled language.
    #[error(
        "path {path} matched multiple languages: {}",
        format_language_list(languages)
    )]
    AmbiguousLanguage {
        /// Ambiguous repository-relative path.
        path: RelativePath,
        /// Matching languages.
        languages: Vec<LanguageId>,
    },
    /// A LOC backend returned a count that cannot be represented.
    #[error("LOC count for {path} is too large to represent: {count}")]
    CodeLineCountOverflow {
        /// Counted source path.
        path: PathBuf,
        /// Backend-reported count.
        count: u64,
    },
}

/// Result alias used by `sprawl-guard` library code.
pub type Result<T> = std::result::Result<T, SprawlError>;

fn format_language_list(languages: &[LanguageId]) -> String {
    languages
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_traversal_incident_count(incidents: &[TraversalIncident]) -> String {
    match incidents.len() {
        1 => "1 incident".to_owned(),
        count => format!("{count} incidents"),
    }
}
