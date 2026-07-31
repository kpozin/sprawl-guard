//! Ratchet violations rendered in check reports.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::ratchet::RatchetCount;
use crate::{LanguageGroupId, RelativePath};

use super::{DirectoryViolationKind, FileViolationKind};

mod kind;
mod limit;

pub use kind::{RatchetIncreasedViolationKind, RatchetStaleViolationKind, RatchetViolationKind};
use kind::{
    directory_ratchet_increased_kind, directory_ratchet_stale_kind, file_ratchet_increased_kind,
    file_ratchet_stale_kind,
};
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
    pub fn kind(&self) -> RatchetViolationKind {
        match self {
            Self::Increased(violation) => violation.kind().into(),
            Self::Stale(violation) => violation.kind().into(),
        }
    }

    /// Returns the repository-relative file or directory path.
    pub fn path(&self) -> &RelativePath {
        if let Some(measured) = self.measured() {
            return measured.path();
        }
        self.missing()
            .expect("unmeasured ratchet diagnostics are missing entries")
            .path()
    }

    /// Returns the current measured value, when the path is still measured.
    pub fn actual(&self) -> Option<RatchetCount> {
        self.measured().map(|measured| measured.actual())
    }

    /// Returns the current configured limit, when the path is still measured.
    pub fn limit(&self) -> Option<RatchetLimit> {
        self.measured().and_then(|measured| measured.limit())
    }

    /// Returns the stored ratchet ceiling.
    pub fn ratchet(&self) -> RatchetCount {
        if let Some(measured) = self.measured() {
            return measured.ratchet();
        }
        self.missing()
            .expect("unmeasured ratchet diagnostics are missing entries")
            .ratchet()
    }

    /// Returns the language group for a directory diagnostic.
    pub fn group(&self) -> Option<&LanguageGroupId> {
        self.measured()
            .and_then(|measured| measured.group())
            .or_else(|| self.missing().and_then(RatchetStaleMissingViolation::group))
    }

    fn measured(&self) -> Option<MeasuredRatchetViolationRef<'_>> {
        match self {
            Self::Increased(RatchetIncreasedViolation::File(violation)) => {
                Some(MeasuredRatchetViolationRef::File {
                    path: &violation.path,
                    actual: violation.actual,
                    limit: RatchetLimit::Finite(violation.limit),
                    ratchet: violation.ratchet,
                })
            }
            Self::Increased(RatchetIncreasedViolation::Directory(violation)) => {
                Some(MeasuredRatchetViolationRef::Directory {
                    path: &violation.path,
                    group: &violation.group,
                    actual: violation.actual,
                    limit: RatchetLimit::Finite(violation.limit),
                    ratchet: violation.ratchet,
                })
            }
            Self::Stale(RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                violation,
            ))) => Some(MeasuredRatchetViolationRef::File {
                path: &violation.path,
                actual: violation.actual,
                limit: RatchetLimit::Finite(violation.limit),
                ratchet: violation.ratchet,
            }),
            Self::Stale(RatchetStaleViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(violation),
            )) => Some(MeasuredRatchetViolationRef::Directory {
                path: &violation.path,
                group: &violation.group,
                actual: violation.actual,
                limit: violation.limit,
                ratchet: violation.ratchet,
            }),
            Self::Stale(RatchetStaleViolation::Missing(_)) => None,
        }
    }

    fn missing(&self) -> Option<&RatchetStaleMissingViolation> {
        match self {
            Self::Stale(RatchetStaleViolation::Missing(violation)) => Some(violation),
            Self::Increased(_) | Self::Stale(RatchetStaleViolation::Measured(_)) => None,
        }
    }
}

impl Serialize for RatchetViolation {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut violation = serializer
            .serialize_struct("RatchetViolation", 5 + usize::from(self.group().is_some()))?;
        violation.serialize_field("kind", &self.kind())?;
        violation.serialize_field("path", self.path())?;
        if let Some(group) = self.group() {
            violation.serialize_field("group", group)?;
        }
        violation.serialize_field("actual", &self.actual())?;
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

impl RatchetIncreasedViolation {
    fn kind(&self) -> RatchetIncreasedViolationKind {
        match self {
            Self::File(violation) => file_ratchet_increased_kind(violation.kind),
            Self::Directory(violation) => directory_ratchet_increased_kind(violation.kind),
        }
    }
}

/// File LOC ratchet increase with a current measurement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRatchetIncreasedViolation {
    /// File role whose LOC ratchet increased.
    pub kind: FileViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Current measured value.
    pub actual: RatchetCount,
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

impl RatchetStaleViolation {
    fn kind(&self) -> RatchetStaleViolationKind {
        match self {
            Self::Measured(violation) => violation.kind(),
            Self::Missing(violation) => violation.kind(),
        }
    }
}

/// Stale ratchet entry for a path that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetStaleMeasuredViolation {
    /// Stale file LOC measurement.
    File(FileRatchetStaleMeasuredViolation),
    /// Stale directory fan-out measurement.
    Directory(DirectoryRatchetStaleMeasuredViolation),
}

impl RatchetStaleMeasuredViolation {
    fn kind(&self) -> RatchetStaleViolationKind {
        match self {
            Self::File(violation) => file_ratchet_stale_kind(violation.kind),
            Self::Directory(violation) => directory_ratchet_stale_kind(violation.kind),
        }
    }
}

/// Stale ratchet entry for a file that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRatchetStaleMeasuredViolation {
    /// File role whose LOC ratchet became stale.
    pub kind: FileViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Current measured value.
    pub actual: RatchetCount,
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

enum MeasuredRatchetViolationRef<'violation> {
    File {
        path: &'violation RelativePath,
        actual: RatchetCount,
        limit: RatchetLimit,
        ratchet: RatchetCount,
    },
    Directory {
        path: &'violation RelativePath,
        group: &'violation LanguageGroupId,
        actual: RatchetCount,
        limit: RatchetLimit,
        ratchet: RatchetCount,
    },
}

impl<'violation> MeasuredRatchetViolationRef<'violation> {
    fn path(&self) -> &'violation RelativePath {
        match self {
            Self::File { path, .. } | Self::Directory { path, .. } => path,
        }
    }

    fn group(&self) -> Option<&'violation LanguageGroupId> {
        match self {
            Self::File { .. } => None,
            Self::Directory { group, .. } => Some(group),
        }
    }

    fn actual(&self) -> RatchetCount {
        match self {
            Self::File { actual, .. } | Self::Directory { actual, .. } => *actual,
        }
    }

    fn limit(&self) -> Option<RatchetLimit> {
        match self {
            Self::File { limit, .. } | Self::Directory { limit, .. } => Some(*limit),
        }
    }

    fn ratchet(&self) -> RatchetCount {
        match self {
            Self::File { ratchet, .. } | Self::Directory { ratchet, .. } => *ratchet,
        }
    }
}

/// Stale ratchet entry for a file or grouped directory that is no longer measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatchetStaleMissingViolation {
    /// Missing file LOC measurement.
    File(FileRatchetStaleMissingViolation),
    /// Missing grouped directory fan-out measurement.
    Directory(DirectoryRatchetStaleMissingViolation),
}

impl RatchetStaleMissingViolation {
    fn kind(&self) -> RatchetStaleViolationKind {
        match self {
            Self::File(violation) => violation.kind,
            Self::Directory(violation) => violation.kind,
        }
    }

    fn path(&self) -> &RelativePath {
        match self {
            Self::File(violation) => &violation.path,
            Self::Directory(violation) => &violation.path,
        }
    }

    fn ratchet(&self) -> RatchetCount {
        match self {
            Self::File(violation) => violation.ratchet,
            Self::Directory(violation) => violation.ratchet,
        }
    }

    fn group(&self) -> Option<&LanguageGroupId> {
        match self {
            Self::File(_) => None,
            Self::Directory(violation) => Some(&violation.group),
        }
    }
}

/// Stale file ratchet whose path is no longer measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRatchetStaleMissingViolation {
    /// Which file-ratchet invariant failed.
    pub kind: RatchetStaleViolationKind,
    /// Repository-relative file path.
    pub path: RelativePath,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}

/// Stale directory ratchet whose group and path are no longer measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryRatchetStaleMissingViolation {
    /// Which directory-ratchet invariant failed.
    pub kind: RatchetStaleViolationKind,
    /// Repository-relative directory path.
    pub path: RelativePath,
    /// Persisted language group.
    pub group: LanguageGroupId,
    /// Stored ratchet ceiling.
    pub ratchet: RatchetCount,
}
