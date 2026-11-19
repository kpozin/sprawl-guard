use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::explain_directory::DirectoryExplanation;
use crate::code_line_counter::LineMeasurement;
use crate::config::{ParsedCodeFileByteLimit, UnreadablePathPolicy};
use crate::{
    DiagnosticDetail, FileByteLimit, LanguageGroupId, LanguageId, RelativePath, RustCfgTestInfo,
    SourceResolvedLimits,
};

/// Production or test role assigned to an included source file.
// Classification assigns this role only after resolving source inclusion and language ownership.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// Production source file.
    Production,
    /// Source file classified as test code.
    Test,
}

impl fmt::Display for SourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Production => formatter.write_str("production"),
            Self::Test => formatter.write_str("test"),
        }
    }
}

/// A source file included by the current resolved configuration.
///
/// Included sources have already passed ignore handling, global excludes,
/// language-specific excludes, and any setting that disables test-file
/// files.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClassifiedSource {
    /// Repository-relative path.
    pub path: RelativePath,
    /// Matched language.
    language: LanguageId,
    /// Language group used for directory direct-file measurements.
    group: LanguageGroupId,
    /// Production/test kind.
    pub kind: SourceKind,
}

impl ClassifiedSource {
    /// Creates a classified source with its resolved language group.
    pub fn new(
        path: RelativePath,
        language: LanguageId,
        group: LanguageGroupId,
        kind: SourceKind,
    ) -> Self {
        Self {
            path,
            language,
            group,
            kind,
        }
    }

    /// Returns the language group used for directory direct-file measurements.
    pub fn group(&self) -> &LanguageGroupId {
        &self.group
    }

    /// Returns the matched language.
    pub fn language(&self) -> LanguageId {
        self.language
    }
}

/// Result of ignore-aware source discovery.
///
/// `sources` contains every classified source that could be discovered.
/// `unreadable_paths` contains the paths below the root that could not be read
/// while looking for additional sources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceDiscoveryReport {
    /// Sources accepted by discovery and classification.
    pub(crate) sources: Vec<ClassifiedSource>,
    /// Recoverable traversal failures collected during discovery.
    pub(crate) unreadable_paths: Vec<UnreadablePath>,
    /// Files, source or not, whose resolved byte limit is finite, sorted by path.
    ///
    /// Empty unless discovery was asked to resolve byte limits.
    pub(crate) byte_limited_files: Vec<ByteLimitedFile>,
}

/// A file whose resolved byte limit is finite, so `check` measures its size.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ByteLimitedFile {
    /// Repository-relative file path.
    pub(crate) path: RelativePath,
    /// Resolved byte limit.
    pub(crate) limit: FileByteLimit,
}

/// Recoverable filesystem traversal failure.
///
/// These failures happen while looking for possible source files, before a
/// specific source file is known to require measurement. They are rendered
/// separately from generic check warnings because they make analysis partial.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UnreadablePath {
    /// Stable failure kind for machine consumers.
    pub kind: UnreadablePathKind,
    /// Severity after applying `unreadable_paths`.
    pub severity: UnreadablePathSeverity,
    /// Repository-relative path when the traversal backend reported one under the root.
    pub path: Option<RelativePath>,
    /// Backend detail suitable for display.
    pub detail: DiagnosticDetail,
}

impl UnreadablePath {
    /// Returns the deterministic sort key used before rendering reports.
    pub(crate) fn sort_key(&self) -> UnreadablePathSortKey<'_> {
        UnreadablePathSortKey {
            path: self.path.as_ref(),
            kind: self.kind,
            detail: self.detail.as_str(),
        }
    }
}

/// Stable unreadable path categories.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnreadablePathKind {
    /// A directory could not be read while walking descendants.
    ReadDirFailed,
    /// A descendant path's file type could not be read.
    FileTypeFailed,
    /// A followed symlink would create a filesystem cycle.
    SymlinkLoop,
    /// A followed symlink target was missing.
    BrokenSymlink,
    /// An ignore file could not be fully parsed.
    IgnoreParseFailed,
    /// A discovered source file could not be read, so its lines were not counted.
    ReadFileFailed,
}

/// Whether a gap in analysis fails the check, from `traversal.unreadable_paths` after CLI overrides.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnreadablePathSeverity {
    /// The gap fails the check once the run has reported everything else.
    Error,
    /// The gap makes analysis partial but does not fail the check by itself.
    Warning,
}

impl From<UnreadablePathPolicy> for UnreadablePathSeverity {
    fn from(policy: UnreadablePathPolicy) -> Self {
        match policy {
            UnreadablePathPolicy::Fail => Self::Error,
            UnreadablePathPolicy::Warn => Self::Warning,
        }
    }
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct UnreadablePathSortKey<'path> {
    path: Option<&'path RelativePath>,
    kind: UnreadablePathKind,
    detail: &'path str,
}

/// Diagnostic details for a source file that discovery would include.
///
/// `source` is the same classification returned by source discovery. Extra
/// fields describe later phases that affect the file during `check`, so
/// `sprawl-guard explain` can answer more than the source/non-source decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SourceExplanation {
    /// Path that does not exist, so it was classified by its path alone and not measured.
    NotFound {
        /// Included source classification.
        source: ClassifiedSource,
    },
    /// Existing source measured with all of its effective limits.
    Measured {
        /// Included source classification.
        source: ClassifiedSource,
        /// Resolved file and directory limits used by `check`.
        resolved_limits: SourceResolvedLimits,
        /// Configured-line measurements and line breakdown.
        measurement: LineMeasurement,
        /// Rust-specific preprocessing status, when the source is Rust.
        rust_cfg_test: Option<RustCfgTestInfo>,
    },
    /// Existing source too large to read, so `check` does not count its lines.
    Uncounted {
        /// Included source classification.
        source: ClassifiedSource,
        /// Resolved file and directory limits used by `check`.
        resolved_limits: SourceResolvedLimits,
        /// Configured `traversal.max_parsed_code_file_bytes`, which the file is larger than.
        max_parsed_code_file_bytes: ParsedCodeFileByteLimit,
    },
}

impl SourceExplanation {
    /// Creates an explanation for a source path that does not exist.
    pub(super) fn not_found(source: ClassifiedSource) -> Self {
        Self::NotFound { source }
    }

    /// Creates an explanation for a successfully measured existing source.
    pub(super) fn measured(
        source: ClassifiedSource,
        resolved_limits: SourceResolvedLimits,
        measurement: LineMeasurement,
        rust_cfg_test: Option<RustCfgTestInfo>,
    ) -> Self {
        Self::Measured {
            source,
            resolved_limits,
            measurement,
            rust_cfg_test,
        }
    }

    /// Returns the included source classification.
    pub fn source(&self) -> &ClassifiedSource {
        match self {
            Self::NotFound { source }
            | Self::Measured { source, .. }
            | Self::Uncounted { source, .. } => source,
        }
    }

    /// Returns resolved limits when this source exists.
    pub fn resolved_limits(&self) -> Option<&SourceResolvedLimits> {
        match self {
            Self::NotFound { .. } => None,
            Self::Measured {
                resolved_limits, ..
            }
            | Self::Uncounted {
                resolved_limits, ..
            } => Some(resolved_limits),
        }
    }

    /// Returns the line measurement for a source whose lines were counted.
    pub fn measurement(&self) -> Option<&LineMeasurement> {
        match self {
            Self::NotFound { .. } | Self::Uncounted { .. } => None,
            Self::Measured { measurement, .. } => Some(measurement),
        }
    }

    /// Returns Rust preprocessing details for a measured Rust source.
    pub fn rust_cfg_test(&self) -> Option<&RustCfgTestInfo> {
        match self {
            Self::NotFound { .. }
            | Self::Uncounted { .. }
            | Self::Measured {
                rust_cfg_test: None,
                ..
            } => None,
            Self::Measured {
                rust_cfg_test: Some(info),
                ..
            } => Some(info),
        }
    }
}

/// Full classification for a single path.
///
/// This is the diagnostic form of source discovery. It keeps negative outcomes
/// distinct so `sprawl-guard explain` can report which rule caused a
/// path not to participate in later checks.
///
/// `S` is what an included source carries. The classifier yields plain [`ClassifiedSource`]
/// values because it never reads the filesystem; `explain` then replaces each with a
/// [`SourceExplanation`] that says whether the file exists and how it measures.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathClassification<S = SourceExplanation> {
    /// The path was ignored by standard ignore-aware traversal.
    Ignored {
        /// Repository-relative path.
        path: RelativePath,
    },
    /// The path was excluded by global file config.
    GloballyExcluded {
        /// Repository-relative path.
        path: RelativePath,
    },
    /// The path is the configured generated ratchet file.
    RatchetExcluded {
        /// Repository-relative path.
        path: RelativePath,
    },
    /// The path is the authored configuration file in use.
    ConfigExcluded {
        /// Repository-relative path.
        path: RelativePath,
    },
    /// The path is an existing directory, measured by the source files directly inside it.
    ///
    /// Only `explain` reports this; source discovery classifies files alone.
    Directory(DirectoryExplanation),
    /// The path exists but is neither a regular file nor a directory, such as a symlink that
    /// traversal does not follow.
    ///
    /// Source discovery checks only regular files. `explain` reports this
    /// separately from `NotSource` so a symlink with a source-like
    /// name is not misreported as a language-pattern miss.
    NonFile {
        /// Repository-relative path.
        path: RelativePath,
    },
    /// The file did not match any enabled language.
    NotSource {
        /// Repository-relative path.
        path: RelativePath,
    },
    /// The path matched multiple enabled languages.
    AmbiguousLanguage {
        /// Repository-relative path.
        path: RelativePath,
        /// Matching languages.
        languages: Vec<LanguageId>,
    },
    /// The file matched one language and was excluded by that language config.
    LanguageExcluded {
        /// Repository-relative path.
        path: RelativePath,
        /// Matched language.
        language: LanguageId,
    },
    /// The file matched multiple languages and every match was excluded.
    ///
    /// `explain` reports this separately from `AmbiguousLanguage` because the
    /// path has no checked source. Showing every matched language keeps the
    /// exclusion reason accurate.
    LanguagesExcluded {
        /// Repository-relative path.
        path: RelativePath,
        /// Matched languages whose exclude globs also matched.
        languages: Vec<LanguageId>,
    },
    /// The path is a test file, but test file checks are disabled.
    TestExcluded {
        /// Repository-relative path.
        path: RelativePath,
        /// Matched language.
        language: LanguageId,
    },
    /// The path is included as a source file.
    Source(S),
}

impl<S> PathClassification<S> {
    /// Returns the path of a file whose size byte limits apply to, using `source_path` to find
    /// an included source's path.
    ///
    /// Byte limits apply to every file the walk visits except the ratchet file, the configuration
    /// file and globally excluded files, whatever its language, so a binary file or a file in a
    /// disabled language is measured too. A file that several languages claim is a configuration
    /// error that stops `check` before it reports anything, but `explain` still shows its size.
    pub(crate) fn byte_limited_path<'path>(
        &'path self,
        source_path: impl FnOnce(&'path S) -> &'path RelativePath,
    ) -> Option<&'path RelativePath> {
        match self {
            Self::Source(source) => Some(source_path(source)),
            Self::NotSource { path }
            | Self::AmbiguousLanguage { path, .. }
            | Self::LanguageExcluded { path, .. }
            | Self::LanguagesExcluded { path, .. }
            | Self::TestExcluded { path, .. } => Some(path),
            Self::Ignored { .. }
            | Self::GloballyExcluded { .. }
            | Self::RatchetExcluded { .. }
            | Self::ConfigExcluded { .. }
            | Self::Directory(_)
            | Self::NonFile { .. } => None,
        }
    }

    /// Replaces an included source's payload with `explain_source`'s result, keeping every
    /// exclusion as is.
    pub(super) fn try_map_source<T>(
        self,
        explain_source: impl FnOnce(S) -> crate::Result<T>,
    ) -> crate::Result<PathClassification<T>> {
        Ok(match self {
            Self::Source(source) => PathClassification::Source(explain_source(source)?),
            Self::Ignored { path } => PathClassification::Ignored { path },
            Self::GloballyExcluded { path } => PathClassification::GloballyExcluded { path },
            Self::RatchetExcluded { path } => PathClassification::RatchetExcluded { path },
            Self::ConfigExcluded { path } => PathClassification::ConfigExcluded { path },
            Self::Directory(directory) => PathClassification::Directory(directory),
            Self::NonFile { path } => PathClassification::NonFile { path },
            Self::NotSource { path } => PathClassification::NotSource { path },
            Self::AmbiguousLanguage { path, languages } => {
                PathClassification::AmbiguousLanguage { path, languages }
            }
            Self::LanguageExcluded { path, language } => {
                PathClassification::LanguageExcluded { path, language }
            }
            Self::LanguagesExcluded { path, languages } => {
                PathClassification::LanguagesExcluded { path, languages }
            }
            Self::TestExcluded { path, language } => {
                PathClassification::TestExcluded { path, language }
            }
        })
    }
}
