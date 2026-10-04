//! Stable categories for ratchet diagnostics.

use serde::Serialize;

use crate::check::report::{DirectoryViolationKind, FileViolationKind};

/// Stable ratchet failure categories.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RatchetViolationKind {
    /// Production file LOC rose above its ratchet ceiling.
    FileLocIncreased,
    /// Path-classified test file LOC rose above its ratchet ceiling.
    TestFileLocIncreased,
    /// Production directory fan-out rose above its ratchet ceiling.
    DirectoryFanoutIncreased,
    /// Path-classified test directory fan-out rose above its ratchet ceiling.
    TestDirectoryFanoutIncreased,
    /// Production file LOC improved or no longer violates policy.
    FileLocStale,
    /// Path-classified test file LOC improved or no longer violates policy.
    TestFileLocStale,
    /// Production directory fan-out improved or no longer violates policy.
    DirectoryFanoutStale,
    /// Path-classified test directory fan-out improved or no longer violates policy.
    TestDirectoryFanoutStale,
}

impl RatchetViolationKind {
    pub(super) const fn increased_file(kind: FileViolationKind) -> Self {
        match kind {
            FileViolationKind::FileLoc => Self::FileLocIncreased,
            FileViolationKind::TestFileLoc => Self::TestFileLocIncreased,
        }
    }

    pub(super) const fn increased_directory(kind: DirectoryViolationKind) -> Self {
        match kind {
            DirectoryViolationKind::DirectoryFanout => Self::DirectoryFanoutIncreased,
            DirectoryViolationKind::TestDirectoryFanout => Self::TestDirectoryFanoutIncreased,
        }
    }

    pub(super) const fn stale_file(kind: FileViolationKind) -> Self {
        match kind {
            FileViolationKind::FileLoc => Self::FileLocStale,
            FileViolationKind::TestFileLoc => Self::TestFileLocStale,
        }
    }

    pub(super) const fn stale_directory(kind: DirectoryViolationKind) -> Self {
        match kind {
            DirectoryViolationKind::DirectoryFanout => Self::DirectoryFanoutStale,
            DirectoryViolationKind::TestDirectoryFanout => Self::TestDirectoryFanoutStale,
        }
    }
}
