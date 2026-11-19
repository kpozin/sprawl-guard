//! Check report domain types.

use std::path::{Path, PathBuf};

use nutype::nutype;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::classification::{SourceKind, UnreadablePath};
use crate::code_line_counter::LineMeasurement;
use crate::rust_cfg::RustCfgTestWarning;
use crate::{
    ConfigSource, DiagnosticDetail, DirectFiles, LanguageGroupId, LanguageId, LineLimit,
    RelativePath,
};

use super::runner::SelectedPaths;

mod coverage;
mod ratchet;
mod summary;

pub use coverage::{AnalysisCoverage, AnalysisGaps, UncountedFile};
pub use ratchet::{
    DirectoryRatchetStaleMeasuredViolation, DirectoryRatchetStaleMissingViolation,
    FileRatchetStaleMeasuredViolation, FileRatchetStaleMissingViolation, RatchetLimit,
    RatchetStaleMeasuredViolation, RatchetStaleMissingViolation, RatchetViolation,
    RatchetViolationKind,
};
pub use summary::{
    CheckedDirectoryCount, CheckedFileCount, DetectedLanguageCount, RatchetEntryCount, ScanSummary,
};

/// Result of one `sprawl-guard check` run that finished.
///
/// A finished run reports every violation it found, along with any paths or files it could not
/// analyze. Run-level failures, such as invalid configuration or an unreadable root, are errors
/// instead.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckReport {
    root: ReportRoot,
    selected_paths: SelectedPaths,
    config_source: ConfigSource,
    counter: CounterInfo,
    coverage: AnalysisCoverage,
    summary: ScanSummary,
    warnings: Vec<CheckWarning>,
    violations: Vec<CheckViolation>,
}

impl Serialize for CheckReport {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut report = serializer.serialize_struct("CheckReport", 9)?;
        report.serialize_field("status", &self.status())?;
        report.serialize_field("root", &self.root)?;
        report.serialize_field("paths", &self.selected_paths)?;
        report.serialize_field("config_source", &self.config_source)?;
        report.serialize_field("counter", &self.counter)?;
        report.serialize_field("coverage", &self.coverage)?;
        report.serialize_field("summary", &self.summary)?;
        report.serialize_field("warnings", &self.warnings)?;
        report.serialize_field("violations", &self.violations)?;
        report.end()
    }
}

impl CheckReport {
    pub(crate) fn new(
        context: CheckContext,
        counter: CounterInfo,
        coverage: AnalysisCoverage,
        summary: ScanSummary,
        warnings: Vec<CheckWarning>,
        violations: Vec<CheckViolation>,
    ) -> Self {
        Self {
            root: context.root,
            selected_paths: context.selected_paths,
            config_source: context.config_source,
            counter,
            coverage,
            summary,
            warnings,
            violations,
        }
    }

    /// Creates a complete check report.
    pub fn complete(
        root: ReportRoot,
        config_source: ConfigSource,
        counter: CounterInfo,
        summary: ScanSummary,
        warnings: Vec<CheckWarning>,
        violations: Vec<CheckViolation>,
    ) -> Self {
        Self::new(
            CheckContext::new(root, SelectedPaths::All, config_source),
            counter,
            AnalysisCoverage::Complete,
            summary,
            warnings,
            violations,
        )
    }

    /// Creates a check report of the whole root with the given coverage.
    pub fn with_coverage(
        root: ReportRoot,
        config_source: ConfigSource,
        counter: CounterInfo,
        summary: ScanSummary,
        warnings: Vec<CheckWarning>,
        coverage: AnalysisCoverage,
        violations: Vec<CheckViolation>,
    ) -> Self {
        Self::new(
            CheckContext::new(root, SelectedPaths::All, config_source),
            counter,
            coverage,
            summary,
            warnings,
            violations,
        )
    }

    /// Returns the derived check status for this report.
    ///
    /// Gaps that fail the check outrank violations, because the violations may be incomplete.
    pub fn status(&self) -> CheckStatus {
        if self.coverage.has_error() {
            CheckStatus::Error
        } else if self.has_violations() {
            CheckStatus::Failed
        } else {
            CheckStatus::Passed
        }
    }

    /// Returns the resolved repository root.
    pub fn root(&self) -> &str {
        self.root.as_str()
    }

    /// Returns the normalized paths selected by the caller.
    pub fn selected_paths(&self) -> &SelectedPaths {
        &self.selected_paths
    }

    /// Returns the source from which the resolved configuration was assembled.
    pub fn config_source(&self) -> &ConfigSource {
        &self.config_source
    }

    /// Returns line-counting backend metadata used to produce measurements.
    pub fn counter(&self) -> &CounterInfo {
        &self.counter
    }

    /// Returns whether discovery and measurement covered every path required within the selected paths.
    pub fn analysis_complete(&self) -> bool {
        matches!(self.coverage, AnalysisCoverage::Complete)
    }

    /// Returns counts describing the source files covered by this check.
    pub const fn summary(&self) -> ScanSummary {
        self.summary
    }

    /// Returns non-fatal warnings emitted during the check.
    pub fn warnings(&self) -> &[CheckWarning] {
        &self.warnings
    }

    /// Returns the paths below the root that could not be read during discovery.
    pub fn unreadable_paths(&self) -> &[UnreadablePath] {
        self.coverage.unreadable_paths()
    }

    /// Returns the source files whose lines were not counted because they are too large.
    pub fn uncounted_files(&self) -> &[UncountedFile] {
        self.coverage.uncounted_files()
    }

    /// Returns the violations found by this run.
    pub fn violations(&self) -> &[CheckViolation] {
        &self.violations
    }

    /// Returns whether the report contains at least one violation.
    pub fn has_violations(&self) -> bool {
        !self.violations.is_empty()
    }
}

/// Selected paths and the configuration source for one completed check.
pub(crate) struct CheckContext {
    root: ReportRoot,
    selected_paths: SelectedPaths,
    config_source: ConfigSource,
}

impl CheckContext {
    pub(crate) fn new(
        root: ReportRoot,
        selected_paths: SelectedPaths,
        config_source: ConfigSource,
    ) -> Self {
        Self {
            root,
            selected_paths,
            config_source,
        }
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

/// Overall check status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// No violations were found, and every gap, if any, only warns.
    Passed,
    /// One or more violations were found, and every gap, if any, only warns.
    Failed,
    /// Some paths or files could not be analyzed under `traversal.unreadable_paths = "fail"`.
    Error,
}

/// Line-counting backend metadata included in stable machine-readable output.
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

/// One violation in a check report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CheckViolation {
    /// A source file exceeded its line limit.
    FileLines(FileLinesViolation),
    /// A directory exceeded its configured direct-file limit.
    DirectoryDirectFiles(DirectoryDirectFilesViolation),
    /// A ratchet entry is stale.
    Ratchet(RatchetViolation),
}

/// A directory that exceeded its configured direct-file limit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DirectoryDirectFilesViolation {
    /// Which directory direct-file limit was exceeded.
    pub kind: DirectoryViolationKind,
    /// Language group whose direct child files exceeded the limit.
    pub group: LanguageGroupId,
    /// Repository-relative directory path.
    ///
    /// The repository root is represented by an empty relative path.
    pub path: RelativePath,
    /// Counted direct child files.
    pub actual: DirectFiles,
    /// Configured direct-file limit.
    pub limit: DirectFiles,
    /// Ceiling recorded for this directory in the ratchet file, which the directory grew above.
    ///
    /// `None` means the ratchet file has no entry for this directory, or the check did not read
    /// a ratchet file.
    pub ratchet: Option<DirectFiles>,
    /// Direct child files that contributed to `actual`.
    pub files: Vec<RelativePath>,
}

/// Directory direct-file count violation kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryViolationKind {
    /// Production direct-file violation.
    DirectoryDirectFiles,
    /// Test direct-file violation.
    TestDirectoryDirectFiles,
}

impl DirectoryViolationKind {
    /// Returns the production or test role measured by this violation.
    pub const fn role(self) -> SourceKind {
        match self {
            Self::DirectoryDirectFiles => SourceKind::Production,
            Self::TestDirectoryDirectFiles => SourceKind::Test,
        }
    }
}

/// A file that exceeded its line limit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileLinesViolation {
    /// Which line limit was exceeded.
    pub kind: FileViolationKind,
    /// Repository-relative source path.
    pub path: RelativePath,
    /// Language used for line counting.
    pub language: LanguageId,
    /// Measured lines and their breakdown.
    #[serde(flatten)]
    pub measurement: LineMeasurement,
    /// Line limit.
    pub limit: LineLimit,
    /// Ceiling recorded for this file in the ratchet file, which the file grew above.
    ///
    /// `None` means the ratchet file has no entry for this file, or the check did not read
    /// a ratchet file.
    pub ratchet: Option<LineLimit>,
}

/// File line violation kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileViolationKind {
    /// Production-path file line violation.
    FileLines,
    /// Test file line violation.
    TestFileLines,
}

impl FileViolationKind {
    /// Returns the production or test role measured by this violation.
    pub const fn role(self) -> SourceKind {
        match self {
            Self::FileLines => SourceKind::Production,
            Self::TestFileLines => SourceKind::Test,
        }
    }
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
