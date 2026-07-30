//! Library support for `sprawl-guard` source-tree analysis.

pub mod check;
pub mod classification;
pub mod code_line_counter;
pub mod config;
pub mod error;
pub mod ratchet;
mod resolved_limits;
mod rust_cfg;
mod source_file;
#[cfg(test)]
mod test_support;
mod value;

pub use check::{
    AnalysisCoverage, CheckErrorKind, CheckOptions, CheckReport, CheckStatus, CheckViolation,
    CheckWarning, CheckWarningKind, DirectoryFanoutViolation, DirectoryViolationKind,
    FileLocViolation, FileViolationKind, NonEmptyTraversalIncidents, RatchetIncreasedViolation,
    RatchetIncreasedViolationKind, RatchetStaleMeasuredViolation, RatchetStaleMissingViolation,
    RatchetStaleViolation, RatchetStaleViolationKind, RatchetViolation, RatchetViolationKind,
    ReportRoot, ReportRootError, check_without_ratchet,
};
pub use classification::{TraversalIncident, TraversalIncidentKind, TraversalIncidentSeverity};
pub use config::{
    ColorMode, OverrideLimits, OverridePaths, OverridePathsError, OverrideReason,
    OverrideReasonError, TraversalErrorPolicy,
};
pub use error::{Result, SprawlError};
pub use ratchet::{
    DirectoryRatchet, FileRatchet, RatchetCount, RatchetFile, RatchetVersion, TestDirectoryRatchet,
    TestFileRatchet,
};
pub use resolved_limits::{
    MatchedOverride, OverrideOrdinal, ResolvedCodeLineLimit, ResolvedLeafFileLimit,
    SourceResolvedLimits,
};
pub use rust_cfg::{ExcludedItemCount, RustCfgTestInfo, RustCfgTestStatus};
pub use value::glob::{GlobPattern, GlobPatternError};
pub use value::language::{LanguageId, LanguageIdError};
pub use value::language_group::{CustomLanguageGroupId, LanguageGroupId, LanguageGroupIdError};
pub use value::limits::{CodeLines, LeafFiles, LimitValueError, LimitValueKind};
pub use value::line_types::{
    CountedLoc, IncludedLineType, IncludedLineTypes, IncludedLineTypesError, LineCount,
    LineCountOverflow, LineCounts,
};
pub use value::path::{RatchetFilePath, RatchetFilePathError, RelativePath};
pub use value::text::DiagnosticDetail;
