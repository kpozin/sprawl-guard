//! Checks over classified and measured sources.

mod directory_direct_files;
mod ratchet;
mod report;
mod runner;

pub use report::{
    AnalysisCoverage, CheckErrorKind, CheckReport, CheckStatus, CheckViolation, CheckWarning,
    CheckWarningKind, CheckedDirectoryCount, CheckedFileCount, CounterInfo, DetectedLanguageCount,
    DirectoryDirectFilesViolation, DirectoryRatchetStaleMeasuredViolation,
    DirectoryRatchetStaleMissingViolation, DirectoryViolationKind, FileLinesViolation,
    FileRatchetStaleMeasuredViolation, FileRatchetStaleMissingViolation, FileViolationKind,
    NonEmptyUnreadablePaths, RatchetEntryCount, RatchetLimit, RatchetStaleMeasuredViolation,
    RatchetStaleMissingViolation, RatchetViolation, RatchetViolationKind, ReportRoot,
    ReportRootError, ScanSummary,
};
pub(crate) use runner::measure_source;
pub use runner::{
    CheckOptions, NonEmptyCheckPaths, SelectedPaths, check, check_with_counter,
    check_without_ratchet,
};
