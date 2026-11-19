//! Library support for `sprawl-guard` source-tree analysis.

pub mod check;
pub mod classification;
pub mod code_line_counter;
pub mod config;
pub mod error;
pub mod file_io;
pub mod ratchet;
mod resolved_limits;
mod rust_cfg;
mod source_file;
#[cfg(test)]
mod test_support;
mod value;

pub use check::{
    AnalysisCoverage, AnalysisGaps, CheckOptions, CheckReport, CheckStatus, CheckViolation,
    CheckWarning, CheckWarningKind, CheckedDirectoryCount, CheckedFileCount, DetectedLanguageCount,
    DirectoryDirectFilesViolation, DirectoryRatchetStaleMeasuredViolation,
    DirectoryRatchetStaleMissingViolation, DirectoryViolationKind,
    FileBytesRatchetStaleMeasuredViolation, FileBytesRatchetStaleMissingViolation,
    FileBytesViolation, FileBytesViolationKind, FileLinesViolation,
    FileRatchetStaleMeasuredViolation, FileRatchetStaleMissingViolation, FileViolationKind,
    NonEmptyCheckPaths, RatchetEntryCount, RatchetLimit, RatchetStaleMeasuredViolation,
    RatchetStaleMissingViolation, RatchetViolation, RatchetViolationKind, ReportRoot,
    ReportRootError, ScanSummary, SelectedPaths, UncountedFile, check_without_ratchet,
};
pub use classification::{UnreadablePath, UnreadablePathKind, UnreadablePathSeverity};
pub use config::{
    AuthoredConfigPath, ConfigSource, OverrideLimits, OverridePaths, OverridePathsError,
    OverrideReason, OverrideReasonError, ResolvedConfig, UnreadablePathPolicy,
};
pub use error::{ConfigUpgradeCommand, Result, SprawlError};
pub use ratchet::{
    DirectoryRatchet, DirectoryRatchetKey, FileRatchet, FileRatchetKey, FileSizeRatchet,
    RatchetCount, RatchetFile, RatchetLookup, RatchetSchemaVersion, authored_ratchet_json_schema,
};
pub use resolved_limits::{
    MatchedOverride, OverrideOrdinal, ResolvedDirectFileLimit, ResolvedFileBytesLimit,
    ResolvedFileLinesLimit, SourceResolvedLimits,
};
pub use rust_cfg::{ExcludedItemCount, RustCfgTestInfo, RustCfgTestStatus};
pub use value::file_bytes::{FileByteLimit, FileByteLimitError, FileBytes, MaxBytesPerFile};
pub use value::glob::{GlobPattern, GlobPatternError};
pub use value::language::{LanguageId, LanguageIdError};
pub use value::language_group::{CustomLanguageGroupId, LanguageGroupId, LanguageGroupIdError};
pub use value::limits::{
    DirectFileLimit, DirectFiles, FiniteOrUnlimited, LimitValueError, LimitValueKind, LineLimit,
    MaxFindings, ParseMaxFindingsError,
};
pub use value::line_types::{
    CountLines, CountLinesError, CountedLines, ExtraLineType, LineCount, LineCountOverflow,
    LineCounts,
};
pub use value::path::{RatchetFilePath, RatchetFilePathError, RelativePath};
pub use value::schema::{TomlSchemaDirective, TomlSchemaUrl, render_authored_schema};
pub use value::text::DiagnosticDetail;
