//! Violations of the byte limit that applies to every file.

use serde::Serialize;

use crate::{FileByteLimit, FileBytes, RelativePath};

/// A file, source or not, that is larger than its byte limit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileBytesViolation {
    /// Stable violation kind for machine consumers.
    pub kind: FileBytesViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Size of the file on disk.
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
    /// The file is larger than `max_bytes_per_file`.
    FileBytes,
}
