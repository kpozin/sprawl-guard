//! Ratchet violations rendered in check reports.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::code_line_counter::LocMeasurement;
use crate::ratchet::RatchetCount;
use crate::{LanguageGroupId, RelativePath};

use super::{DirectoryViolationKind, FileViolationKind};

mod kind;
mod limit;

pub use kind::RatchetViolationKind;
pub use limit::RatchetLimit;

/// A ratchet entry that failed current-check semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetViolation {
    /// A measured value increased above its stored ratchet ceiling.
    Increased(RatchetIncreasedViolation),
    /// A ratchet entry is no longer current.
    Stale(RatchetStaleViolation),
}

impl RatchetViolation {
    /// Returns the stable violation kind rendered in machine-readable output.
    pub const fn kind(&self) -> RatchetViolationKind {
        match self {
            Self::Increased(RatchetIncreasedViolation::File(violation)) => {
                RatchetViolationKind::increased_file(violation.kind)
            }
            Self::Increased(RatchetIncreasedViolation::Directory(violation)) => {
                RatchetViolationKind::increased_directory(violation.kind)
            }
            Self::Stale(RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                violation,
            ))) => RatchetViolationKind::stale_file(violation.kind),
            Self::Stale(RatchetStaleViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(violation),
            )) => RatchetViolationKind::stale_directory(violation.kind),
            Self::Stale(RatchetStaleViolation::Missing(RatchetStaleMissingViolation::File(
                violation,
            ))) => RatchetViolationKind::stale_file(violation.kind),
            Self::Stale(RatchetStaleViolation::Missing(
                RatchetStaleMissingViolation::Directory(violation),
            )) => RatchetViolationKind::stale_directory(violation.kind),
        }
    }

    /// Returns the repository-relative file or directory path.
    pub fn path(&self) -> &RelativePath {
        match self {
            Self::Increased(RatchetIncreasedViolation::File(violation)) => &violation.path,
            Self::Increased(RatchetIncreasedViolation::Directory(violation)) => &violation.path,
            Self::Stale(RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                violation,
            ))) => &violation.path,
            Self::Stale(RatchetStaleViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(violation),
            )) => &violation.path,
            Self::Stale(RatchetStaleViolation::Missing(RatchetStaleMissingViolation::File(
                violation,
            ))) => &violation.path,
            Self::Stale(RatchetStaleViolation::Missing(
                RatchetStaleMissingViolation::Directory(violation),
            )) => &violation.path,
        }
    }

    /// Returns the current measured value, when the path is still measured.
    pub fn actual(&self) -> Option<RatchetCount> {
        match self {
            Self::Increased(RatchetIncreasedViolation::File(violation)) => Some(
                RatchetCount::from_counted_loc(violation.measurement.actual()),
            ),
            Self::Increased(RatchetIncreasedViolation::Directory(violation)) => {
                Some(violation.actual)
            }
            Self::Stale(RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                violation,
            ))) => Some(RatchetCount::from_counted_loc(
                violation.measurement.actual(),
            )),
            Self::Stale(RatchetStaleViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(violation),
            )) => Some(violation.actual),
            Self::Stale(RatchetStaleViolation::Missing(_)) => None,
        }
    }

    /// Returns the current configured limit, when the path is still measured.
    pub fn limit(&self) -> Option<RatchetLimit> {
        match self {
            Self::Increased(RatchetIncreasedViolation::File(violation)) => {
                Some(RatchetLimit::Finite(violation.limit))
            }
            Self::Increased(RatchetIncreasedViolation::Directory(violation)) => {
                Some(RatchetLimit::Finite(violation.limit))
            }
            Self::Stale(RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                violation,
            ))) => Some(RatchetLimit::Finite(violation.limit)),
            Self::Stale(RatchetStaleViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(violation),
            )) => Some(violation.limit),
            Self::Stale(RatchetStaleViolation::Missing(_)) => None,
        }
    }

    /// Returns the stored ratchet ceiling.
    pub const fn ratchet(&self) -> RatchetCount {
        match self {
            Self::Increased(RatchetIncreasedViolation::File(violation)) => violation.ratchet,
            Self::Increased(RatchetIncreasedViolation::Directory(violation)) => violation.ratchet,
            Self::Stale(RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                violation,
            ))) => violation.ratchet,
            Self::Stale(RatchetStaleViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(violation),
            )) => violation.ratchet,
            Self::Stale(RatchetStaleViolation::Missing(RatchetStaleMissingViolation::File(
                violation,
            ))) => violation.ratchet,
            Self::Stale(RatchetStaleViolation::Missing(
                RatchetStaleMissingViolation::Directory(violation),
            )) => violation.ratchet,
        }
    }

    /// Returns the language group for a directory diagnostic.
    pub fn group(&self) -> Option<&LanguageGroupId> {
        match self {
            Self::Increased(RatchetIncreasedViolation::Directory(violation)) => {
                Some(&violation.group)
            }
            Self::Stale(RatchetStaleViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(violation),
            )) => Some(&violation.group),
            Self::Stale(RatchetStaleViolation::Missing(
                RatchetStaleMissingViolation::Directory(violation),
            )) => Some(&violation.group),
            Self::Increased(RatchetIncreasedViolation::File(_))
            | Self::Stale(RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                _,
            )))
            | Self::Stale(RatchetStaleViolation::Missing(RatchetStaleMissingViolation::File(_))) => {
                None
            }
        }
    }

    /// Returns the configured-LOC measurement for a measured file diagnostic.
    pub fn loc_measurement(&self) -> Option<&LocMeasurement> {
        match self {
            Self::Increased(RatchetIncreasedViolation::File(violation)) => {
                Some(&violation.measurement)
            }
            Self::Stale(RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                violation,
            ))) => Some(&violation.measurement),
            Self::Increased(RatchetIncreasedViolation::Directory(_))
            | Self::Stale(RatchetStaleViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(_),
            ))
            | Self::Stale(RatchetStaleViolation::Missing(_)) => None,
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
        if let Some(measurement) = self.loc_measurement() {
            violation.serialize_field("included_line_types", &measurement.included_line_types())?;
            violation.serialize_field("line_counts", &measurement.line_counts())?;
        }
        violation.serialize_field("limit", &self.limit())?;
        violation.serialize_field("ratchet", &self.ratchet())?;
        violation.end()
    }
}

/// Ratchet increase with a current measured value and finite configured limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetIncreasedViolation {
    /// File LOC ratchet increase.
    File(FileRatchetIncreasedViolation),
    /// Directory fan-out ratchet increase.
    Directory(DirectoryRatchetIncreasedViolation),
}

/// File LOC ratchet increase with a current measurement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRatchetIncreasedViolation {
    /// File role whose LOC ratchet increased.
    pub kind: FileViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Current configured-LOC measurement.
    pub measurement: LocMeasurement,
    /// Current configured limit.
    pub limit: RatchetCount,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Directory fan-out ratchet increase with a current grouped measurement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryRatchetIncreasedViolation {
    /// Directory role whose fan-out ratchet increased.
    pub kind: DirectoryViolationKind,
    /// Repository-relative directory path.
    pub path: RelativePath,
    /// Language group measured at the directory.
    pub group: LanguageGroupId,
    /// Current measured value.
    pub actual: RatchetCount,
    /// Current configured limit.
    pub limit: RatchetCount,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Ratchet entry that is no longer current.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetStaleViolation {
    /// The path is still measured, but the stored ceiling is stale.
    Measured(RatchetStaleMeasuredViolation),
    /// The path is no longer measured.
    Missing(RatchetStaleMissingViolation),
}

/// Stale ratchet entry for a path that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetStaleMeasuredViolation {
    /// Stale file LOC measurement.
    File(FileRatchetStaleMeasuredViolation),
    /// Stale directory fan-out measurement.
    Directory(DirectoryRatchetStaleMeasuredViolation),
}

/// Stale ratchet entry for a file that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRatchetStaleMeasuredViolation {
    /// File role whose LOC ratchet became stale.
    pub kind: FileViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Current configured-LOC measurement.
    pub measurement: LocMeasurement,
    /// Current configured limit.
    pub limit: RatchetCount,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Stale ratchet entry for a directory that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryRatchetStaleMeasuredViolation {
    /// Directory role whose fan-out ratchet became stale.
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
    /// Missing file LOC measurement.
    File(FileRatchetStaleMissingViolation),
    /// Missing grouped directory fan-out measurement.
    Directory(DirectoryRatchetStaleMissingViolation),
}

/// Stale file ratchet whose path is no longer measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRatchetStaleMissingViolation {
    /// File role whose LOC ratchet became stale.
    pub kind: FileViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Stale directory ratchet whose group and path are no longer measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryRatchetStaleMissingViolation {
    /// Directory role whose fan-out ratchet became stale.
    pub kind: DirectoryViolationKind,
    /// Repository-relative directory path.
    pub path: RelativePath,
    /// Persisted language group.
    pub group: LanguageGroupId,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}
