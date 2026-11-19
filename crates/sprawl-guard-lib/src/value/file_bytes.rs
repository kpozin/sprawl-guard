//! File sizes measured on disk.

use nutype::nutype;

/// Size of a file on disk, in bytes.
#[nutype(derive(
    Clone, Copy, Debug, Display, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize
))]
pub struct FileBytes(u64);
