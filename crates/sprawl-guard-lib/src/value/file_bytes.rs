//! File sizes measured on disk, and the limits they are checked against.

use std::fmt;

use nutype::nutype;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

/// How a file's content is stored, which decides the byte limit that applies to it.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FileStorage {
    /// The file's content is stored as is, so `max_bytes_per_file` limits its size on disk.
    Regular,
    /// Git LFS stores the file's content and the repository holds only a small pointer to it, so
    /// `max_bytes_per_lfs_object` limits the size of the content.
    Lfs,
}

impl FileStorage {
    /// Every way a file can be stored.
    pub(crate) const ALL: [Self; 2] = [Self::Regular, Self::Lfs];
}

impl fmt::Display for FileStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Regular => "regular",
            Self::Lfs => "lfs",
        })
    }
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
