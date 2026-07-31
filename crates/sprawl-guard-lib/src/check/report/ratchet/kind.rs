//! Stable categories for ratchet diagnostics.

use serde::Serialize;

use crate::check::report::{DirectoryViolationKind, FileViolationKind};

/// Stable ratchet increase categories.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RatchetIncreasedViolationKind {
    /// Production file LOC rose above its ratchet ceiling.
    FileLocIncreased,
    /// Path-classified test file LOC rose above its ratchet ceiling.
    TestFileLocIncreased,
    /// Production directory fan-out rose above its ratchet ceiling.
    DirectoryFanoutIncreased,
    /// Path-classified test directory fan-out rose above its ratchet ceiling.
    TestDirectoryFanoutIncreased,
}

/// Stable stale ratchet categories.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RatchetStaleViolationKind {
    /// Production file LOC improved or no longer violates policy.
    FileLocStale,
    /// Path-classified test file LOC improved or no longer violates policy.
    TestFileLocStale,
    /// Production directory fan-out improved or no longer violates policy.
    DirectoryFanoutStale,
    /// Path-classified test directory fan-out improved or no longer violates policy.
    TestDirectoryFanoutStale,
}

pub(super) fn file_ratchet_increased_kind(
    kind: FileViolationKind,
) -> RatchetIncreasedViolationKind {
    match kind {
        FileViolationKind::FileLoc => RatchetIncreasedViolationKind::FileLocIncreased,
        FileViolationKind::TestFileLoc => RatchetIncreasedViolationKind::TestFileLocIncreased,
    }
}

pub(super) fn directory_ratchet_increased_kind(
    kind: DirectoryViolationKind,
) -> RatchetIncreasedViolationKind {
    match kind {
        DirectoryViolationKind::DirectoryFanout => {
            RatchetIncreasedViolationKind::DirectoryFanoutIncreased
        }
        DirectoryViolationKind::TestDirectoryFanout => {
            RatchetIncreasedViolationKind::TestDirectoryFanoutIncreased
        }
    }
}

pub(super) fn file_ratchet_stale_kind(kind: FileViolationKind) -> RatchetStaleViolationKind {
    match kind {
        FileViolationKind::FileLoc => RatchetStaleViolationKind::FileLocStale,
        FileViolationKind::TestFileLoc => RatchetStaleViolationKind::TestFileLocStale,
    }
}

pub(super) fn directory_ratchet_stale_kind(
    kind: DirectoryViolationKind,
) -> RatchetStaleViolationKind {
    match kind {
        DirectoryViolationKind::DirectoryFanout => RatchetStaleViolationKind::DirectoryFanoutStale,
        DirectoryViolationKind::TestDirectoryFanout => {
            RatchetStaleViolationKind::TestDirectoryFanoutStale
        }
    }
}

impl From<RatchetIncreasedViolationKind> for RatchetViolationKind {
    fn from(value: RatchetIncreasedViolationKind) -> Self {
        match value {
            RatchetIncreasedViolationKind::FileLocIncreased => Self::FileLocIncreased,
            RatchetIncreasedViolationKind::TestFileLocIncreased => Self::TestFileLocIncreased,
            RatchetIncreasedViolationKind::DirectoryFanoutIncreased => {
                Self::DirectoryFanoutIncreased
            }
            RatchetIncreasedViolationKind::TestDirectoryFanoutIncreased => {
                Self::TestDirectoryFanoutIncreased
            }
        }
    }
}

impl From<RatchetStaleViolationKind> for RatchetViolationKind {
    fn from(value: RatchetStaleViolationKind) -> Self {
        match value {
            RatchetStaleViolationKind::FileLocStale => Self::FileLocStale,
            RatchetStaleViolationKind::TestFileLocStale => Self::TestFileLocStale,
            RatchetStaleViolationKind::DirectoryFanoutStale => Self::DirectoryFanoutStale,
            RatchetStaleViolationKind::TestDirectoryFanoutStale => Self::TestDirectoryFanoutStale,
        }
    }
}

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
