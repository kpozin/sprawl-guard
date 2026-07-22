use serde::Serialize;

use crate::{DiagnosticDetail, LanguageId, RelativePath, RustCfgTestInfo, SourceResolvedLimits};

/// Production/test kind for an included source file.
///
/// `SourceKind` is not the source/non-source decision. It is assigned only
/// after a path has already been accepted as a source file for an enabled
/// language.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// Production source file.
    Production,
    /// Source file classified as test code.
    Test,
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
    pub language: LanguageId,
    /// Production/test kind.
    pub kind: SourceKind,
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
pub struct SourceExplanation {
    /// Included source classification.
    pub source: ClassifiedSource,
    /// Resolved file and directory limits used by `check`, when available.
    ///
    /// Path-only explanations for missing files do not have this field because
    /// no filesystem-backed source was available for later check-phase
    /// diagnostics.
    pub resolved_limits: Option<SourceResolvedLimits>,
    /// Rust-specific preprocessing status, when the source is a Rust file.
    pub rust_cfg_test: Option<RustCfgTestInfo>,
}

impl SourceExplanation {
    /// Creates an explanation before later-phase diagnostics are attached.
    pub(super) fn classified(source: ClassifiedSource) -> Self {
        Self {
            source,
            resolved_limits: None,
            rust_cfg_test: None,
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
