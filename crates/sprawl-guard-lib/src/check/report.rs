//! Check report domain types.

use std::path::{Path, PathBuf};

use nutype::nutype;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::classification::TraversalIncident;
use crate::code_line_counter::LocMeasurement;
use crate::rust_cfg::RustCfgTestWarning;
use crate::{DiagnosticDetail, LanguageGroupId, LanguageId, LeafFiles, LocLimit, RelativePath};

mod ratchet;

pub use ratchet::{
    DirectoryRatchetIncreasedViolation, DirectoryRatchetStaleMeasuredViolation,
    DirectoryRatchetStaleMissingViolation, FileRatchetIncreasedViolation,
    FileRatchetStaleMeasuredViolation, FileRatchetStaleMissingViolation, RatchetIncreasedViolation,
    RatchetIncreasedViolationKind, RatchetLimit, RatchetStaleMeasuredViolation,
    RatchetStaleMissingViolation, RatchetStaleViolation, RatchetStaleViolationKind,
    RatchetViolation, RatchetViolationKind,
};

/// Successful result of one `sprawl-guard check` run.
///
/// This type represents completed policy analysis only. Operational failures,
/// including fail-mode traversal incidents, are returned as errors instead of
/// being embedded in the report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckReport {
    root: ReportRoot,
    counter: CounterInfo,
    coverage: AnalysisCoverage,
    warnings: Vec<CheckWarning>,
    violations: Vec<CheckViolation>,
}

impl Serialize for CheckReport {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut report = serializer.serialize_struct("CheckReport", 6)?;
        report.serialize_field("status", &self.status())?;
        report.serialize_field("root", &self.root)?;
        report.serialize_field("counter", &self.counter)?;
        report.serialize_field("coverage", &self.coverage)?;
        report.serialize_field("warnings", &self.warnings)?;
        report.serialize_field("violations", &self.violations)?;
        report.end()
    }
}

impl CheckReport {
    pub(crate) fn new(
        root: ReportRoot,
        counter: CounterInfo,
        coverage: AnalysisCoverage,
        warnings: Vec<CheckWarning>,
        violations: Vec<CheckViolation>,
    ) -> Self {
        Self {
            root,
            counter,
            coverage,
            warnings,
            violations,
        }
    }

    /// Creates a complete check report.
    pub fn complete(
        root: ReportRoot,
        counter: CounterInfo,
        warnings: Vec<CheckWarning>,
        violations: Vec<CheckViolation>,
    ) -> Self {
        Self::new(
            root,
            counter,
            AnalysisCoverage::Complete,
            warnings,
            violations,
        )
    }

    /// Creates a partial check report from non-empty traversal incidents.
    pub fn partial(
        root: ReportRoot,
        counter: CounterInfo,
        warnings: Vec<CheckWarning>,
        traversal_incidents: NonEmptyTraversalIncidents,
        violations: Vec<CheckViolation>,
    ) -> Self {
        Self::new(
            root,
            counter,
            AnalysisCoverage::Partial {
                traversal_incidents,
            },
            warnings,
            violations,
        )
    }

    /// Returns the derived policy status for this report.
    pub fn status(&self) -> CheckStatus {
        if self.has_violations() {
            CheckStatus::Failed
        } else {
            CheckStatus::Passed
        }
    }

    /// Returns the resolved repository root.
    pub fn root(&self) -> &str {
        self.root.as_str()
    }

    /// Returns LOC backend metadata used to produce measurements.
    pub fn counter(&self) -> &CounterInfo {
        &self.counter
    }

    /// Returns whether discovery and measurement covered every path required by config.
    pub fn analysis_complete(&self) -> bool {
        matches!(self.coverage, AnalysisCoverage::Complete)
    }

    /// Returns non-fatal warnings emitted during the check.
    pub fn warnings(&self) -> &[CheckWarning] {
        &self.warnings
    }

    /// Returns recoverable descendant traversal failures collected during discovery.
    pub fn traversal_incidents(&self) -> &[TraversalIncident] {
        match &self.coverage {
            AnalysisCoverage::Complete => &[],
            AnalysisCoverage::Partial {
                traversal_incidents,
            } => traversal_incidents.as_slice(),
        }
    }

    /// Returns policy violations found by this run.
    pub fn violations(&self) -> &[CheckViolation] {
        &self.violations
    }

    /// Returns whether the report contains at least one policy violation.
    pub fn has_violations(&self) -> bool {
        !self.violations.is_empty()
    }
}

/// Resolved repository root rendered in check reports.
#[nutype(
    validate(with = validate_report_root, error = ReportRootError),
    derive(Clone, Debug, Eq, PartialEq, Serialize, AsRef, Display),
)]
pub struct ReportRoot(String);

impl ReportRoot {
    /// Creates a report root from an already-resolved absolute root path.
    pub(crate) fn from_resolved_path(root: &Path) -> Self {
        debug_assert!(root.is_absolute(), "report root must be absolute");
        Self::try_new(root.display().to_string()).expect("resolved report root must be absolute")
    }

    /// Creates a report root from an absolute path.
    pub fn new(root: PathBuf) -> std::result::Result<Self, ReportRootError> {
        Self::try_new(root.display().to_string())
    }

    /// Creates a report root from a trusted absolute string.
    pub fn trusted(root: &'static str) -> Self {
        Self::try_new(root).expect("trusted report root must be absolute")
    }

    /// Returns the display form used in reports.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

fn validate_report_root(root: &str) -> std::result::Result<(), ReportRootError> {
    if !Path::new(root).is_absolute() {
        return Err(ReportRootError::Relative {
            path: PathBuf::from(root),
        });
    }
    Ok(())
}

/// Error returned when a report root is not an absolute path.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReportRootError {
    /// Root path was relative.
    #[error("report root must be absolute: {path}")]
    Relative {
        /// Invalid root path.
        path: PathBuf,
    },
}

/// Whether a check result covered all paths or returned partial analysis.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalysisCoverage {
    /// All required paths were discovered and measured.
    Complete,
    /// Analysis continued after recoverable traversal incidents.
    Partial {
        /// Non-empty incident list explaining why analysis is partial.
        traversal_incidents: NonEmptyTraversalIncidents,
    },
}

/// Non-empty traversal incident collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NonEmptyTraversalIncidents(Vec<TraversalIncident>);

impl NonEmptyTraversalIncidents {
    /// Creates a non-empty incident collection.
    pub fn new(incidents: Vec<TraversalIncident>) -> Option<Self> {
        if incidents.is_empty() {
            return None;
        }
        Some(Self(incidents))
    }

    /// Returns the incidents as a slice.
    pub fn as_slice(&self) -> &[TraversalIncident] {
        &self.0
    }
}

/// Overall check status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// No violations were found.
    Passed,
    /// One or more violations were found.
    Failed,
}

/// LOC backend metadata included in stable machine-readable output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CounterInfo {
    /// Backend identifier.
    pub backend: &'static str,
    /// Backend version.
    pub version: &'static str,
}

/// Non-fatal check warning included in human and machine-readable output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CheckWarning {
    /// Stable warning kind for machine consumers.
    pub kind: CheckWarningKind,
    /// Repository-relative path affected by the warning.
    pub path: RelativePath,
    /// Backend or parser detail suitable for display.
    pub detail: DiagnosticDetail,
}

impl CheckWarning {
    pub(crate) fn rust_cfg_test_parse_failed(
        path: RelativePath,
        warning: RustCfgTestWarning,
    ) -> Self {
        Self {
            kind: CheckWarningKind::RustCfgTestParseFailed,
            path,
            detail: warning.detail,
        }
    }
}

/// Stable warning categories emitted by checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckWarningKind {
    /// Rust parsing failed, so cfg(test) preprocessing fell back to original source.
    RustCfgTestParseFailed,
}

/// Stable operational error categories emitted by checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckErrorKind {
    /// Recoverable traversal incidents were collected in fail mode.
    TraversalFailed,
}

/// One policy violation in a check report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CheckViolation {
    /// A source file exceeded its configured LOC budget.
    FileLoc(FileLocViolation),
    /// A directory exceeded its configured direct leaf-file budget.
    DirectoryFanout(DirectoryFanoutViolation),
    /// A ratchet entry increased or became stale.
    Ratchet(RatchetViolation),
}

/// A directory that exceeded its configured direct leaf-file budget.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DirectoryFanoutViolation {
    /// Which directory fan-out budget was exceeded.
    pub kind: DirectoryViolationKind,
    /// Language group whose direct child files exceeded the budget.
    pub group: LanguageGroupId,
    /// Repository-relative directory path.
    ///
    /// The repository root is represented by an empty relative path.
    pub path: RelativePath,
    /// Counted direct child leaf files.
    pub actual: LeafFiles,
    /// Configured leaf-file limit.
    pub limit: LeafFiles,
    /// Ratchet limit that covers this violation, when available.
    ///
    /// `None` means no ratchet baseline was evaluated or no ratchet entry
    /// covered this directory.
    pub ratchet: Option<LeafFiles>,
    /// Direct child leaf files that contributed to `actual`.
    pub files: Vec<RelativePath>,
}

/// Directory fan-out violation kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryViolationKind {
    /// Production leaf-file fan-out violation.
    DirectoryFanout,
    /// Path-classified test leaf-file fan-out violation.
    TestDirectoryFanout,
}

/// A file that exceeded its configured LOC budget.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileLocViolation {
    /// Which LOC budget was exceeded.
    pub kind: FileViolationKind,
    /// Repository-relative source path.
    pub path: RelativePath,
    /// Language used for LOC counting.
    pub language: LanguageId,
    /// Measured configured LOC and its logical-line breakdown.
    #[serde(flatten)]
    pub measurement: LocMeasurement,
    /// Configured LOC limit.
    pub limit: LocLimit,
    /// Ratchet limit that covers this violation, when available.
    ///
    /// `None` means no ratchet baseline was evaluated or no ratchet entry
    /// covered this file.
    pub ratchet: Option<LocLimit>,
}

/// File LOC violation kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileViolationKind {
    /// Production-path file LOC violation.
    FileLoc,
    /// Path-classified test file LOC violation.
    TestFileLoc,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod report_root {
        use super::*;

        mod when_path_is_absolute {
            use super::*;

            #[test]
            fn it_constructs_the_report_root() {
                let root = ReportRoot::new(std::env::current_dir().unwrap()).unwrap();

                assert!(Path::new(root.as_str()).is_absolute());
            }
        }

        mod when_path_is_relative {
            use super::*;

            #[test]
            fn it_rejects_the_report_root() {
                let error = ReportRoot::new(PathBuf::from("repo")).unwrap_err();

                assert_eq!(
                    error,
                    ReportRootError::Relative {
                        path: PathBuf::from("repo"),
                    }
                );
            }
        }
    }
}
