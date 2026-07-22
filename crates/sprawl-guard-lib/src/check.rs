//! Policy checks over classified and measured sources.

mod directory_fanout;
mod ratchet;
mod report;
mod runner;

pub use report::{
    AnalysisCoverage, CheckErrorKind, CheckReport, CheckStatus, CheckViolation, CheckWarning,
    CheckWarningKind, CounterInfo, DirectoryFanoutViolation, DirectoryViolationKind,
    FileLocViolation, FileViolationKind, NonEmptyTraversalIncidents, RatchetIncreasedViolation,
    RatchetIncreasedViolationKind, RatchetStaleMeasuredViolation, RatchetStaleMissingViolation,
    RatchetStaleViolation, RatchetStaleViolationKind, RatchetViolation, RatchetViolationKind,
    ReportRoot, ReportRootError,
};
pub use runner::{CheckOptions, check, check_with_counter, check_without_ratchet};
