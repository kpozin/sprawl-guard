//! Stable categories for ratchet diagnostics.

use serde::Serialize;

use crate::check::report::{DirectoryViolationKind, FileViolationKind};

/// Stable categories of stale ratchet entries.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RatchetViolationKind {
    /// Production file line count improved or no longer exceeds its limit.
    FileLinesStale,
    /// Test file line count improved or no longer exceeds its limit.
    TestFileLinesStale,
    /// Production directory direct-file count improved or no longer exceeds its limit.
    DirectoryDirectFilesStale,
    /// Test direct-file count improved or no longer exceeds its limit.
    TestDirectoryDirectFilesStale,
}

impl RatchetViolationKind {
    pub(super) const fn stale_file(kind: FileViolationKind) -> Self {
        match kind {
            FileViolationKind::FileLines => Self::FileLinesStale,
            FileViolationKind::TestFileLines => Self::TestFileLinesStale,
        }
    }

    pub(super) const fn stale_directory(kind: DirectoryViolationKind) -> Self {
        match kind {
            DirectoryViolationKind::DirectoryDirectFiles => Self::DirectoryDirectFilesStale,
            DirectoryViolationKind::TestDirectoryDirectFiles => Self::TestDirectoryDirectFilesStale,
        }
    }
}
