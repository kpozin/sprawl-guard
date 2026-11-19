//! Error and result types for the library.

use std::path::PathBuf;

use crate::classification::{SourceKind, UnreadablePath};
use crate::config::{ConfigSchemaVersion, LanguageGroupsError};
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
    /// A repository path cannot be represented by the UTF-8 logical path model.
    #[error("repository path is not valid UTF-8: {path:?}")]
    NonUtf8Path {
        /// Invalid native path.
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
    /// A requested selected path does not exist.
    #[error("selected path does not exist: {path}")]
    MissingSelectedPath {
        /// Missing selected path as supplied by the caller.
        path: PathBuf,
    },
    /// An explicit config file does not exist.
    #[error("config file does not exist: {path}")]
    MissingConfig {
        /// Missing config path.
        path: PathBuf,
    },
    /// A selected config path could not be represented in structured output.
    #[error("config path is not valid UTF-8: {path:?}")]
    NonUtf8ConfigPath {
        /// Selected config path.
        path: PathBuf,
    },
    /// A config file could not be read.
    #[error("failed to read {path}: {source}")]
    ReadConfig {
        /// Path that could not be read.
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
    /// A source file could not be read for line counting.
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
    /// A validated root config could not be represented as an editable TOML document.
    #[error("failed to parse editable TOML document {path}: {source}")]
    ParseEditableToml {
        /// TOML file path.
        path: PathBuf,
        /// Formatting-preserving TOML parser error.
        source: toml_edit::TomlError,
    },
    /// A root config file omitted its required schema version.
    #[error("config file is missing required schema_version")]
    MissingConfigSchemaVersion,
    /// A root config file used a non-integer schema version.
    #[error("config schema_version must be an integer")]
    InvalidConfigSchemaVersionType,
    /// A root config file uses an older schema version, which `config upgrade` can migrate.
    #[error(
        "{} uses schema version {version}; this version reads version {}.\nRun `{upgrade}` to upgrade it.",
        path.display(),
        ConfigSchemaVersion::CURRENT
    )]
    OutdatedConfigSchemaVersion {
        /// Root config file path, relative to the current directory when it is under it.
        path: PathBuf,
        /// Command that upgrades this file when run from the current directory.
        upgrade: ConfigUpgradeCommand,
        /// Version read from the root config file.
        version: ConfigSchemaVersion,
    },
    /// A root config file used a schema version this operation cannot accept.
    #[error("unsupported config schema version {version}; expected current version 2")]
    UnsupportedConfigSchemaVersion {
        /// Version read from the root config file.
        version: i64,
    },
    /// A ratchet TOML file could not be parsed.
    #[error("failed to parse ratchet file {path}: {source}")]
    ParseRatchet {
        /// Ratchet file path.
        path: PathBuf,
        /// TOML parser error.
        source: toml::de::Error,
    },
    /// A ratchet file omitted its required schema version.
    #[error("ratchet file is missing required schema_version = 1")]
    MissingRatchetSchemaVersion,
    /// A ratchet file used an unsupported numeric schema version.
    #[error("unsupported ratchet schema version {version}; expected version 1")]
    UnsupportedRatchetSchemaVersion {
        /// Version read from the ratchet file.
        version: i64,
    },
    /// A file ratchet entry used the repository root instead of a file path.
    #[error("file ratchet entry path must not be empty")]
    EmptyFileRatchetPath,
    /// A ratchet file contained a duplicate file identity.
    #[error("duplicate file ratchet entry for role {role} and path {path}")]
    DuplicateFileRatchetEntry {
        /// Duplicate source role.
        role: SourceKind,
        /// Duplicate ratchet path.
        path: RelativePath,
    },
    /// A ratchet file contained a duplicate directory identity.
    #[error("duplicate directory ratchet entry for role {role}, group {group}, and path {path}")]
    DuplicateDirectoryRatchetEntry {
        /// Duplicate source role.
        role: SourceKind,
        /// Duplicate language group.
        group: LanguageGroupId,
        /// Duplicate directory path.
        path: RelativePath,
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
        "enabled language {language} must use bundled paths, a nonempty include, or nonempty extend_include"
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
    /// Ignore-aware traversal found unreadable paths.
    #[error("could not read {} path(s) under {root}", unreadable_paths.len())]
    TraversalFailed {
        /// Traversed repository root.
        root: PathBuf,
        /// Collected unreadable paths.
        unreadable_paths: Vec<UnreadablePath>,
    },
    /// Source discovery inspected more filesystem entries than allowed.
    #[error("traversal entry limit exceeded under {root}: saw more than {limit} entries")]
    TraversalEntryLimitExceeded {
        /// Traversed repository root.
        root: PathBuf,
        /// Configured entry limit.
        limit: u32,
    },
    /// A source file was too large for its code lines to be parsed and counted.
    #[error(
        "source {path} is too large to count: {size} bytes exceeds traversal.max_parsed_code_file_bytes ({limit})"
    )]
    ParsedCodeFileTooLarge {
        /// Source file path.
        path: PathBuf,
        /// File size in bytes.
        size: u64,
        /// Configured `traversal.max_parsed_code_file_bytes`.
        limit: crate::config::ParsedCodeFileByteLimit,
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
    /// A line-counting backend returned a count that cannot be represented.
    #[error("line count for {path} is too large to represent: {count}")]
    CodeLineCountOverflow {
        /// Counted source path.
        path: PathBuf,
        /// Backend-reported count.
        count: u64,
    },
}

/// The `config upgrade` command line that finds a particular config file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigUpgradeCommand {
    /// The file is `sprawl-guard.toml` in the current directory, which the command finds itself.
    InCurrentDirectory,
    /// The file is elsewhere, so the command names it with `--config`.
    WithConfig(PathBuf),
}

impl std::fmt::Display for ConfigUpgradeCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("sprawl-guard config upgrade --write")?;
        match self {
            Self::InCurrentDirectory => Ok(()),
            Self::WithConfig(path) => {
                write!(
                    formatter,
                    " --config {}",
                    shell_word(&path.to_string_lossy())
                )
            }
        }
    }
}

/// Quotes `word` for a POSIX shell when it contains anything other than characters that are
/// always safe there, so the user can paste the command as is.
fn shell_word(word: &str) -> std::borrow::Cow<'_, str> {
    let safe =
        |character: char| character.is_ascii_alphanumeric() || "-_./+:@%,".contains(character);
    if !word.is_empty() && word.chars().all(safe) {
        word.into()
    } else {
        format!("'{}'", word.replace('\'', r"'\''")).into()
    }
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
