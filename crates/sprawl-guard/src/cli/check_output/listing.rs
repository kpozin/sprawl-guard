//! Which findings the human report lists under the finding cap, and in what order.

use sprawl_guard_lib::CheckReport;
use sprawl_guard_lib::check::{CheckViolation, DirectoryViolationKind, FileViolationKind};
use sprawl_guard_lib::{FiniteOrUnlimited, MaxFindings};

/// How many entries of each section the human report lists under the finding cap.
///
/// Sections draw from one budget, failures first so that a failing run always shows why it
/// failed: violations, then warnings, then unreadable paths. The report still prints them in its
/// own order. Sections that `--quiet` hides neither count toward the total nor draw from the
/// budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FindingListing {
    pub(super) warnings: usize,
    pub(super) violations: usize,
    pub(super) unreadable_paths: usize,
    /// How many member files each listed directory finding names.
    pub(super) directory_files: usize,
    /// Findings in every printed section, listed or not.
    pub(super) total: usize,
}

impl FindingListing {
    pub(super) fn new(
        report: &CheckReport,
        quiet: bool,
        max_findings: FiniteOrUnlimited<MaxFindings>,
    ) -> Self {
        let limit = entry_limit(max_findings);
        let mut remaining = limit;
        let mut total = 0;
        let mut list = |count: usize| {
            total += count;
            let listed = count.min(remaining);
            remaining -= listed;
            listed
        };
        let (warnings, unreadable_paths) = if quiet {
            (0, 0)
        } else {
            (report.warnings().len(), report.unreadable_paths().len())
        };
        let violations = list(report.violations().len());
        let warnings = list(warnings);
        let unreadable_paths = list(unreadable_paths);
        Self {
            warnings,
            violations,
            unreadable_paths,
            directory_files: limit,
            total,
        }
    }

    pub(super) const fn listed(self) -> usize {
        self.warnings + self.violations + self.unreadable_paths
    }
}

/// The most entries one list may show under the finding cap.
pub(super) fn entry_limit(max_findings: FiniteOrUnlimited<MaxFindings>) -> usize {
    max_findings.finite().map_or(usize::MAX, |max| {
        usize::try_from(max.get()).unwrap_or(usize::MAX)
    })
}

/// A violation section of the human report. Sections print in the order declared here.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ViolationGroup {
    FileLines,
    TestFileLines,
    DirectoryDirectFiles,
    TestDirectoryDirectFiles,
    StaleRatchet,
}

impl ViolationGroup {
    pub(super) fn of(violation: &CheckViolation) -> Self {
        match violation {
            CheckViolation::FileLines(violation) => match violation.kind {
                FileViolationKind::FileLines => Self::FileLines,
                FileViolationKind::TestFileLines => Self::TestFileLines,
            },
            CheckViolation::DirectoryDirectFiles(violation) => match violation.kind {
                DirectoryViolationKind::DirectoryDirectFiles => Self::DirectoryDirectFiles,
                DirectoryViolationKind::TestDirectoryDirectFiles => Self::TestDirectoryDirectFiles,
            },
            CheckViolation::Ratchet(_) => Self::StaleRatchet,
        }
    }

    pub(super) const fn heading(self) -> &'static str {
        match self {
            Self::FileLines => "Files over the line limit",
            Self::TestFileLines => "Test files over the line limit",
            Self::DirectoryDirectFiles => "Directories with too many direct files",
            Self::TestDirectoryDirectFiles => "Test directories with too many direct files",
            Self::StaleRatchet => "Stale ratchet entries",
        }
    }
}

/// Returns the report's violations in the order the human report lists them, keeping the
/// report's order within each group.
pub(super) fn violations_in_display_order(report: &CheckReport) -> Vec<&CheckViolation> {
    let mut violations = report.violations().iter().collect::<Vec<_>>();
    violations.sort_by_key(|violation| ViolationGroup::of(violation));
    violations
}
