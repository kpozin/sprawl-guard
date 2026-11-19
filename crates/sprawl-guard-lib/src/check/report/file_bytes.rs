//! Violations of the byte limit that applies to every file.

use serde::Serialize;

use crate::{FileByteLimit, FileBytes, FileStorage, RelativePath};

/// A file, source or not, that is larger than its byte limit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileBytesViolation {
    /// Stable violation kind for machine consumers, which says which byte limit the file is over.
    pub kind: FileBytesViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Size of the file's content. For a file in Git LFS that isn't downloaded, it is the size
    /// that its pointer declares.
    pub actual: FileBytes,
    /// Byte limit that applies to the file.
    pub limit: FileByteLimit,
    /// Ceiling recorded for this file in the ratchet file, which the file grew above.
    ///
    /// `None` means the ratchet file has no byte entry for this file, or the check did not read
    /// a ratchet file.
    pub ratchet: Option<FileByteLimit>,
}

/// File byte violation kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileBytesViolationKind {
    /// A file that is not in Git LFS is larger than `max_bytes_per_file`.
    FileBytes,
    /// A file that Git LFS stores is larger than `max_bytes_per_lfs_object`.
    LfsObjectBytes,
}

impl FileBytesViolationKind {
    /// Returns how a file with this kind of violation is stored.
    pub const fn storage(self) -> FileStorage {
        match self {
            Self::FileBytes => FileStorage::Regular,
            Self::LfsObjectBytes => FileStorage::Lfs,
        }
    }
}

impl From<FileStorage> for FileBytesViolationKind {
    fn from(storage: FileStorage) -> Self {
        match storage {
            FileStorage::Regular => Self::FileBytes,
            FileStorage::Lfs => Self::LfsObjectBytes,
        }
    }
}
