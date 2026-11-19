//! The order in which a check report lists violations.

use crate::RelativePath;

use super::super::report::{
    CheckViolation, DirectoryViolationKind, FileBytesViolationKind, FileViolationKind,
    RatchetViolationKind,
};

/// Sorts violations by kind, file violations first and stale ratchet entries last, then by path.
pub(super) fn sort_violations(violations: &mut [CheckViolation]) {
    violations.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
}

impl CheckViolation {
    fn sort_key(&self) -> ViolationSortKey<'_> {
        match self {
            Self::FileLines(violation) => {
                ViolationSortKey::new(file_violation_sort_order(violation.kind), &violation.path)
            }
            Self::DirectoryDirectFiles(violation) => ViolationSortKey::new(
                directory_violation_sort_order(violation.kind),
                &violation.path,
            ),
            Self::FileBytes(violation) => {
                ViolationSortKey::new(file_bytes_sort_order(violation.kind), &violation.path)
            }
            Self::Ratchet(violation) => ViolationSortKey::new(
                ratchet_violation_sort_order(violation.kind()),
                violation.path(),
            ),
        }
    }
}

/// Position of a violation in the report: its kind's rank, then its path.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct ViolationSortKey<'path> {
    kind_order: u8,
    path: &'path RelativePath,
}

impl<'path> ViolationSortKey<'path> {
    fn new(kind_order: u8, path: &'path RelativePath) -> Self {
        Self { kind_order, path }
    }
}

fn file_violation_sort_order(kind: FileViolationKind) -> u8 {
    match kind {
        FileViolationKind::FileLines => 0,
        FileViolationKind::TestFileLines => 1,
    }
}

/// File size violations follow line violations, since both describe single files.
fn file_bytes_sort_order(kind: FileBytesViolationKind) -> u8 {
    match kind {
        FileBytesViolationKind::FileBytes => 2,
        FileBytesViolationKind::LfsObjectBytes => 3,
    }
}

fn directory_violation_sort_order(kind: DirectoryViolationKind) -> u8 {
    match kind {
        DirectoryViolationKind::DirectoryDirectFiles => 4,
        DirectoryViolationKind::TestDirectoryDirectFiles => 5,
    }
}

fn ratchet_violation_sort_order(kind: RatchetViolationKind) -> u8 {
    match kind {
        RatchetViolationKind::FileLinesStale => 6,
        RatchetViolationKind::TestFileLinesStale => 7,
        RatchetViolationKind::DirectoryDirectFilesStale => 8,
        RatchetViolationKind::TestDirectoryDirectFilesStale => 9,
        RatchetViolationKind::FileBytesStale => 10,
        RatchetViolationKind::LfsObjectBytesStale => 11,
    }
}
