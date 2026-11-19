//! Checks over classified and measured sources.

mod directory_direct_files;
mod file_bytes;
mod ratchet;
mod report;
mod runner;

pub(crate) use ratchet::RatchetState;
pub use report::{
    AnalysisCoverage, AnalysisGaps, CheckReport, CheckStatus, CheckViolation, CheckWarning,
    CheckWarningKind, CheckedDirectoryCount, CheckedFileCount, CounterInfo, DetectedLanguageCount,
    DirectoryDirectFilesViolation, DirectoryRatchetStaleMeasuredViolation,
    DirectoryRatchetStaleMissingViolation, DirectoryViolationKind,
    FileBytesRatchetStaleMeasuredViolation, FileBytesRatchetStaleMissingViolation,
    FileBytesViolation, FileBytesViolationKind, FileLinesViolation,
    FileRatchetStaleMeasuredViolation, FileRatchetStaleMissingViolation, FileViolationKind,
    RatchetEntryCount, RatchetLimit, RatchetStaleMeasuredViolation, RatchetStaleMissingViolation,
    RatchetViolation, RatchetViolationKind, ReportRoot, ReportRootError, ScanSummary,
    UncountedFile,
};
pub(crate) use runner::measure_source;
pub use runner::{
    CheckOptions, NonEmptyCheckPaths, SelectedPaths, check, check_with_counter,
    check_without_ratchet,
};
