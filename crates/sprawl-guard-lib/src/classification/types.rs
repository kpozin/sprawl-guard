use std::fmt;

use serde::{Deserialize, Serialize};

use crate::code_line_counter::LocMeasurement;
use crate::{
    DiagnosticDetail, LanguageGroupId, LanguageId, RelativePath, RustCfgTestInfo,
    SourceResolvedLimits,
};

/// Production/test kind for an included source file.
///
/// `SourceKind` is not the source/non-source decision. It is assigned only
/// after a path has already been accepted as a source file for an enabled
/// language.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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
/// language-specific excludes, and any policy that disables path-classified test
/// files.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClassifiedSource {
    /// Repository-relative path.
    pub path: RelativePath,
    /// Matched language.
    language: LanguageId,
    /// Language group used for directory fan-out measurements.
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

    /// Returns the language group used for directory fan-out measurements.
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
/// `traversal_incidents` contains recoverable descendant traversal failures
/// encountered while looking for additional sources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceDiscoveryReport {
    /// Sources accepted by discovery and classification.
    pub(crate) sources: Vec<ClassifiedSource>,
    /// Recoverable traversal failures collected during discovery.
    pub(crate) traversal_incidents: Vec<TraversalIncident>,
}

/// Recoverable filesystem traversal failure.
///
/// These incidents happen while looking for possible source files, before a
/// specific source file is known to require measurement. They are rendered
/// separately from generic check warnings because they make analysis partial.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TraversalIncident {
    /// Stable incident kind for machine consumers.
    pub kind: TraversalIncidentKind,
    /// Severity after applying traversal error policy.
    pub severity: TraversalIncidentSeverity,
    /// Repository-relative path when the traversal backend reported one under the root.
    pub path: Option<RelativePath>,
    /// Backend detail suitable for display.
    pub detail: DiagnosticDetail,
}

impl TraversalIncident {
    /// Returns the deterministic sort key used before rendering reports.
    pub(crate) fn sort_key(&self) -> TraversalIncidentSortKey<'_> {
        TraversalIncidentSortKey {
            path: self.path.as_ref(),
            kind: self.kind,
            detail: self.detail.as_str(),
        }
    }
}

/// Stable traversal incident categories.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalIncidentKind {
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
    /// Discovery exceeded the configured entry limit.
    EntryLimitExceeded,
}

/// Severity assigned to a traversal incident after config and CLI overrides.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TraversalIncidentSeverity {
    /// Incident will fail the run after discovery finishes.
    Error,
    /// Incident makes analysis partial but does not fail the run by itself.
    Warning,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct TraversalIncidentSortKey<'incident> {
    path: Option<&'incident RelativePath>,
    kind: TraversalIncidentKind,
    detail: &'incident str,
}

/// Diagnostic details for a source file that discovery would include.
///
/// `source` is the same classification returned by source discovery. Extra
/// fields describe later phases that affect the file during `check`, so
/// `sprawl-guard explain` can answer more than the source/non-source decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SourceExplanation {
    /// Classification-only result for a path that was not measured.
    Classified {
        /// Included source classification.
        source: ClassifiedSource,
    },
    /// Existing source measured with its complete effective policy.
    Measured {
        /// Included source classification.
        source: ClassifiedSource,
        /// Resolved file and directory limits used by `check`.
        resolved_limits: SourceResolvedLimits,
        /// Configured-LOC measurement and logical-line breakdown.
        measurement: LocMeasurement,
        /// Rust-specific preprocessing status, when the source is Rust.
        rust_cfg_test: Option<RustCfgTestInfo>,
    },
}

impl SourceExplanation {
    /// Creates an explanation before later-phase diagnostics are attached.
    pub(super) fn classified(source: ClassifiedSource) -> Self {
        Self::Classified { source }
    }

    /// Creates an explanation for a successfully measured existing source.
    pub(super) fn measured(
        source: ClassifiedSource,
        resolved_limits: SourceResolvedLimits,
        measurement: LocMeasurement,
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
            Self::Classified { source } | Self::Measured { source, .. } => source,
        }
    }

    /// Returns resolved limits when this existing source was measured.
    pub fn resolved_limits(&self) -> Option<&SourceResolvedLimits> {
        match self {
            Self::Classified { .. } => None,
            Self::Measured {
                resolved_limits, ..
            } => Some(resolved_limits),
        }
    }

    /// Returns the configured-LOC measurement for an existing source.
    pub fn measurement(&self) -> Option<&LocMeasurement> {
        match self {
            Self::Classified { .. } => None,
            Self::Measured { measurement, .. } => Some(measurement),
        }
    }

    /// Returns Rust preprocessing details for a measured Rust source.
    pub fn rust_cfg_test(&self) -> Option<&RustCfgTestInfo> {
        match self {
            Self::Classified { .. }
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

    /// Consumes the explanation and returns its source classification.
    pub(super) fn into_source(self) -> ClassifiedSource {
        match self {
            Self::Classified { source } | Self::Measured { source, .. } => source,
        }
    }
}

/// Full classification for a single path.
///
/// This is the diagnostic form of source discovery. It keeps negative outcomes
/// distinct so `sprawl-guard explain` can report which policy decision caused a
/// path not to participate in later checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathClassification {
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
    /// The path exists but is not a regular file.
    ///
    /// Source discovery checks only regular files. `explain` reports this
    /// separately from `NotSource` so a directory or symlink with a source-like
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
    /// The path is a source file classified as test code, but path-based test
    /// checks are disabled.
    TestExcluded {
        /// Repository-relative path.
        path: RelativePath,
        /// Matched language.
        language: LanguageId,
    },
    /// The path is included as a source file.
    Source(SourceExplanation),
}
