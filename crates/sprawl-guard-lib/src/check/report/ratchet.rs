//! Ratchet violations rendered in check reports.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::code_line_counter::LineMeasurement;
use crate::ratchet::RatchetCount;
use crate::{FileByteLimit, FileBytes, FileStorage, LanguageGroupId, RelativePath};

use super::{DirectoryViolationKind, FileViolationKind};

mod kind;
mod limit;

pub use kind::RatchetViolationKind;
pub use limit::RatchetLimit;

/// A ratchet entry that is no longer current: its violation shrank, ended, or disappeared.
///
/// A violation that grew above its ceiling is reported as a limit violation that carries the
/// ceiling, not as a ratchet violation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetViolation {
    /// The path is still measured, but the stored ceiling is above its current measurement.
    Measured(RatchetStaleMeasuredViolation),
    /// The path is no longer measured.
    Missing(RatchetStaleMissingViolation),
}

impl RatchetViolation {
    /// Returns the stable violation kind rendered in machine-readable output.
    pub const fn kind(&self) -> RatchetViolationKind {
        match self {
            Self::Measured(RatchetStaleMeasuredViolation::File(violation)) => {
                RatchetViolationKind::stale_file(violation.kind)
            }
            Self::Measured(RatchetStaleMeasuredViolation::Directory(violation)) => {
                RatchetViolationKind::stale_directory(violation.kind)
            }
            Self::Missing(RatchetStaleMissingViolation::File(violation)) => {
                RatchetViolationKind::stale_file(violation.kind)
            }
            Self::Missing(RatchetStaleMissingViolation::Directory(violation)) => {
                RatchetViolationKind::stale_directory(violation.kind)
            }
            Self::Measured(RatchetStaleMeasuredViolation::FileBytes(violation)) => {
                RatchetViolationKind::stale_file_bytes(violation.storage)
            }
            Self::Missing(RatchetStaleMissingViolation::FileBytes(violation)) => {
                RatchetViolationKind::stale_file_bytes(violation.storage)
            }
        }
    }

    /// Returns the repository-relative file or directory path.
    pub fn path(&self) -> &RelativePath {
        match self {
            Self::Measured(RatchetStaleMeasuredViolation::File(violation)) => &violation.path,
            Self::Measured(RatchetStaleMeasuredViolation::Directory(violation)) => &violation.path,
            Self::Missing(RatchetStaleMissingViolation::File(violation)) => &violation.path,
            Self::Missing(RatchetStaleMissingViolation::Directory(violation)) => &violation.path,
            Self::Measured(RatchetStaleMeasuredViolation::FileBytes(violation)) => &violation.path,
            Self::Missing(RatchetStaleMissingViolation::FileBytes(violation)) => &violation.path,
        }
    }

    /// Returns the current measured value, when the path is still measured.
    pub fn actual(&self) -> Option<RatchetCount> {
        match self {
            Self::Measured(RatchetStaleMeasuredViolation::File(violation)) => Some(
                RatchetCount::from_counted_lines(violation.measurement.actual()),
            ),
            Self::Measured(RatchetStaleMeasuredViolation::Directory(violation)) => {
                Some(violation.actual)
            }
            Self::Measured(RatchetStaleMeasuredViolation::FileBytes(violation)) => {
                Some(RatchetCount::from_file_bytes(violation.actual))
            }
            Self::Missing(_) => None,
        }
    }

    /// Returns the current configured limit, when the path is still measured.
    pub fn limit(&self) -> Option<RatchetLimit> {
        match self {
            Self::Measured(RatchetStaleMeasuredViolation::File(violation)) => {
                Some(RatchetLimit::Finite(violation.limit))
            }
            Self::Measured(RatchetStaleMeasuredViolation::Directory(violation)) => {
                Some(violation.limit)
            }
            Self::Measured(RatchetStaleMeasuredViolation::FileBytes(violation)) => Some(
                RatchetLimit::Finite(RatchetCount::from_file_byte_limit(violation.limit)),
            ),
            Self::Missing(_) => None,
        }
    }

    /// Returns the stored ratchet ceiling.
    pub fn ratchet(&self) -> RatchetCount {
        match self {
            Self::Measured(RatchetStaleMeasuredViolation::File(violation)) => violation.ratchet,
            Self::Measured(RatchetStaleMeasuredViolation::Directory(violation)) => {
                violation.ratchet
            }
            Self::Missing(RatchetStaleMissingViolation::File(violation)) => violation.ratchet,
            Self::Missing(RatchetStaleMissingViolation::Directory(violation)) => violation.ratchet,
            Self::Measured(RatchetStaleMeasuredViolation::FileBytes(violation)) => {
                RatchetCount::from_file_byte_limit(violation.ratchet)
            }
            Self::Missing(RatchetStaleMissingViolation::FileBytes(violation)) => {
                RatchetCount::from_file_byte_limit(violation.ratchet)
            }
        }
    }

    /// Returns the language group for a directory diagnostic.
    pub fn group(&self) -> Option<&LanguageGroupId> {
        match self {
            Self::Measured(RatchetStaleMeasuredViolation::Directory(violation)) => {
                Some(&violation.group)
            }
            Self::Missing(RatchetStaleMissingViolation::Directory(violation)) => {
                Some(&violation.group)
            }
            Self::Measured(
                RatchetStaleMeasuredViolation::File(_)
                | RatchetStaleMeasuredViolation::FileBytes(_),
            )
            | Self::Missing(
                RatchetStaleMissingViolation::File(_) | RatchetStaleMissingViolation::FileBytes(_),
            ) => None,
        }
    }

    /// Returns the line measurement for a measured file diagnostic.
    pub fn line_measurement(&self) -> Option<&LineMeasurement> {
        match self {
            Self::Measured(RatchetStaleMeasuredViolation::File(violation)) => {
                Some(&violation.measurement)
            }
            Self::Measured(
                RatchetStaleMeasuredViolation::Directory(_)
                | RatchetStaleMeasuredViolation::FileBytes(_),
            )
            | Self::Missing(_) => None,
        }
    }
}

impl Serialize for RatchetViolation {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut violation = serializer.serialize_struct("RatchetViolation", 9)?;
        violation.serialize_field("kind", &self.kind())?;
        violation.serialize_field("path", self.path())?;
        if let Some(group) = self.group() {
            violation.serialize_field("group", group)?;
        }
        violation.serialize_field("actual", &self.actual())?;
        if let Some(measurement) = self.line_measurement() {
            violation.serialize_field("count_lines", &measurement.count_lines())?;
            violation.serialize_field("line_counts", &measurement.line_counts())?;
        }
        violation.serialize_field("limit", &self.limit())?;
        violation.serialize_field("ratchet", &self.ratchet())?;
        violation.end()
    }
}

/// Stale ratchet entry for a path that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetStaleMeasuredViolation {
    /// Stale file line measurements.
    File(FileRatchetStaleMeasuredViolation),
    /// Stale directory direct-file measurement.
    Directory(DirectoryRatchetStaleMeasuredViolation),
    /// Stale file size measurement.
    FileBytes(FileBytesRatchetStaleMeasuredViolation),
}

/// Stale ratchet entry for a file that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRatchetStaleMeasuredViolation {
    /// File role whose line ratchet became stale.
    pub kind: FileViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Current line measurement.
    pub measurement: LineMeasurement,
    /// Current configured limit.
    pub limit: RatchetCount,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Stale ratchet entry for a directory that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryRatchetStaleMeasuredViolation {
    /// Directory role whose direct-file count ratchet became stale.
    pub kind: DirectoryViolationKind,
    /// Repository-relative directory path.
    pub path: RelativePath,
    /// Language group measured at the directory.
    pub group: LanguageGroupId,
    /// Current measured value.
    pub actual: RatchetCount,
    /// Current configured finite or unlimited limit.
    pub limit: RatchetLimit,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Stale ratchet entry for a file or grouped directory that is no longer measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetStaleMissingViolation {
    /// Missing file line measurements.
    File(FileRatchetStaleMissingViolation),
    /// Missing grouped directory direct-file measurement.
    Directory(DirectoryRatchetStaleMissingViolation),
    /// Missing file size measurement.
    FileBytes(FileBytesRatchetStaleMissingViolation),
}

/// Stale file ratchet whose path is no longer measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRatchetStaleMissingViolation {
    /// File role whose line ratchet became stale.
    pub kind: FileViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Stale directory ratchet whose group and path are no longer measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryRatchetStaleMissingViolation {
    /// Directory role whose direct-file count ratchet became stale.
    pub kind: DirectoryViolationKind,
    /// Repository-relative directory path.
    pub path: RelativePath,
    /// Persisted language group.
    pub group: LanguageGroupId,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Stale byte ratchet for a file whose size is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileBytesRatchetStaleMeasuredViolation {
    /// Repository-relative file path.
    pub path: RelativePath,
    /// How the file is stored, which decides its byte limit.
    pub storage: FileStorage,
    /// Current size of the file.
    pub actual: FileBytes,
    /// Byte limit that currently applies to the file.
    pub limit: FileByteLimit,
    /// Stored ratchet ceiling.
    pub ratchet: FileByteLimit,
}

/// Stale byte ratchet for a file whose size is no longer measured: the file is gone, excluded, or
/// no longer has a byte limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileBytesRatchetStaleMissingViolation {
    /// Repository-relative file path.
    pub path: RelativePath,
    /// How the file was stored when its ceiling was recorded.
    pub storage: FileStorage,
    /// Stored ratchet ceiling.
    pub ratchet: FileByteLimit,
}
