//! File sizes measured on disk, and the limits they are checked against.

use nutype::nutype;
use schemars::JsonSchema;
use serde::Serialize;

use super::limits::FiniteOrUnlimited;

/// Size of a file on disk, in bytes.
#[nutype(derive(
    Clone, Copy, Debug, Display, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize
))]
pub struct FileBytes(u64);

impl FileBytes {
    /// Returns this size as a byte limit, or `None` for an empty file.
    ///
    /// The ratchet records the size of a file over its limit as that file's ceiling.
    pub fn as_limit(self) -> Option<FileByteLimit> {
        FileByteLimit::try_new(self.into_inner()).ok()
    }
}

/// Size of a file that its byte limit is checked against, and where that size came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct MeasuredFileSize {
    /// Size checked against the limit.
    pub bytes: FileBytes,
    /// Where the size came from.
    #[serde(flatten)]
    pub source: FileSizeSource,
}

/// Where a measured file size came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "measured_from", rename_all = "snake_case")]
pub enum FileSizeSource {
    /// The size of the file on disk.
    OnDisk,
    /// The size a Git LFS pointer declares for the file it stands in for, so that a checkout
    /// with and without LFS content measure the same.
    LfsPointer {
        /// Size of the pointer file itself on disk.
        pointer_bytes: FileBytes,
    },
}

/// Error returned when a file byte limit is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FileByteLimitError {
    /// The limit must allow non-empty files.
    #[error("a file byte limit must be greater than zero")]
    Zero,
}

const fn validate_file_byte_limit(value: &u64) -> Result<(), FileByteLimitError> {
    if *value == 0 {
        Err(FileByteLimitError::Zero)
    } else {
        Ok(())
    }
}

/// Largest size, in bytes, that a file may have.
///
/// Configuration uses it for `max_bytes_per_file`, and the ratchet file for the ceiling of a file
/// that was already too large.
#[nutype(
    const_fn,
    validate(with = validate_file_byte_limit, error = FileByteLimitError),
    derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Display, Serialize, Deserialize),
    derive_unchecked(JsonSchema),
)]
pub struct FileByteLimit(#[schemars(range(min = 1))] u64);

impl FileByteLimit {
    /// Returns whether a file of `size` bytes is over this limit.
    pub fn is_exceeded_by(self, size: FileBytes) -> bool {
        size.into_inner() > self.into_inner()
    }

    /// Returns whether this limit is larger than a file of `size` bytes needs.
    pub fn is_above(self, size: FileBytes) -> bool {
        self.into_inner() > size.into_inner()
    }
}

/// Byte limit for one file: a positive number of bytes, or unlimited.
pub type MaxBytesPerFile = FiniteOrUnlimited<FileByteLimit>;
