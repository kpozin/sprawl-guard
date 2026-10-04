//! Policy checks over classified and measured sources.

mod directory_fanout;
mod ratchet;
mod report;
mod runner;

pub use report::{
    AnalysisCoverage, CheckErrorKind, CheckReport, CheckStatus, CheckViolation, CheckWarning,
    CheckWarningKind, CheckedDirectoryCount, CheckedFileCount, CounterInfo, DetectedLanguageCount,
    DirectoryFanoutViolation, DirectoryRatchetIncreasedViolation,
    DirectoryRatchetStaleMeasuredViolation, DirectoryRatchetStaleMissingViolation,
    DirectoryViolationKind, FileLocViolation, FileRatchetIncreasedViolation,
    FileRatchetStaleMeasuredViolation, FileRatchetStaleMissingViolation, FileViolationKind,
    NonEmptyTraversalIncidents, RatchetIncreasedViolation, RatchetLimit,
    RatchetStaleMeasuredViolation, RatchetStaleMissingViolation, RatchetStaleViolation,
    RatchetViolation, RatchetViolationKind, ReportRoot, ReportRootError, ScanSummary,
};
pub(crate) use runner::measure_source;
pub use runner::{
    CheckOptions, CheckScope, NonEmptyCheckPaths, check, check_with_counter, check_without_ratchet,
};
