//! Policy checks over classified and measured sources.

mod directory_fanout;
mod ratchet;
mod report;
mod runner;

pub use report::{
    AnalysisCoverage, CheckErrorKind, CheckReport, CheckStatus, CheckViolation, CheckWarning,
    CheckWarningKind, CounterInfo, DirectoryFanoutViolation, DirectoryRatchetIncreasedViolation,
    DirectoryRatchetStaleMeasuredViolation, DirectoryRatchetStaleMissingViolation,
    DirectoryViolationKind, FileLocViolation, FileRatchetIncreasedViolation,
    FileRatchetStaleMeasuredViolation, FileRatchetStaleMissingViolation, FileViolationKind,
    NonEmptyTraversalIncidents, RatchetIncreasedViolation, RatchetIncreasedViolationKind,
    RatchetLimit, RatchetStaleMeasuredViolation, RatchetStaleMissingViolation,
    RatchetStaleViolation, RatchetStaleViolationKind, RatchetViolation, RatchetViolationKind,
    ReportRoot, ReportRootError,
};
pub(crate) use runner::measure_source;
pub use runner::{CheckOptions, check, check_with_counter, check_without_ratchet};
