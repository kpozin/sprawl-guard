//! Ratchet violations rendered in check reports.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::ratchet::RatchetCount;
use crate::{LanguageGroupId, RelativePath};

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
            Self::Increased(violation) => violation.kind.into(),
            Self::Stale(violation) => violation.kind().into(),
        }
    }

    /// Returns the repository-relative file or directory path.
    pub fn path(&self) -> &RelativePath {
        match self {
            Self::Increased(violation) => &violation.path,
            Self::Stale(violation) => violation.path(),
        }
    }

    /// Returns the current measured value, when the path is still measured.
    pub fn actual(&self) -> Option<RatchetCount> {
        match self {
            Self::Increased(violation) => Some(violation.actual),
            Self::Stale(violation) => violation.actual(),
        }
    }

    /// Returns the current configured limit, when finite and applicable.
    pub fn limit(&self) -> Option<RatchetCount> {
        match self {
            Self::Increased(violation) => Some(violation.limit),
            Self::Stale(violation) => violation.limit(),
        }
    }

    /// Returns the stored ratchet ceiling.
    pub fn ratchet(&self) -> RatchetCount {
        match self {
            Self::Increased(violation) => violation.ratchet,
            Self::Stale(violation) => violation.ratchet(),
        }
    }

    /// Returns the persisted group for a missing directory ratchet.
    pub fn group(&self) -> Option<&LanguageGroupId> {
        match self {
            Self::Increased(_) | Self::Stale(RatchetStaleViolation::Measured(_)) => None,
            Self::Stale(RatchetStaleViolation::Missing(violation)) => violation.group(),
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
pub struct RatchetIncreasedViolation {
    /// Which ratchet increase invariant failed.
    pub kind: RatchetIncreasedViolationKind,
    /// Repository-relative file or directory path.
    pub path: RelativePath,
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
            Self::Measured(violation) => violation.kind,
            Self::Missing(violation) => violation.kind(),
        }
    }

    fn path(&self) -> &RelativePath {
        match self {
            Self::Measured(violation) => &violation.path,
            Self::Missing(violation) => violation.path(),
        }
    }

    fn actual(&self) -> Option<RatchetCount> {
        match self {
            Self::Measured(violation) => Some(violation.actual),
            Self::Missing(_) => None,
        }
    }

    fn limit(&self) -> Option<RatchetCount> {
        match self {
            Self::Measured(violation) => violation.limit,
            Self::Missing(_) => None,
        }
    }

    fn ratchet(&self) -> RatchetCount {
        match self {
            Self::Measured(violation) => violation.ratchet,
            Self::Missing(violation) => violation.ratchet(),
        }
    }
}

/// Stale ratchet entry for a path that is still measured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatchetStaleMeasuredViolation {
    /// Which stale ratchet invariant failed.
    pub kind: RatchetStaleViolationKind,
    /// Repository-relative file or directory path.
    pub path: RelativePath,
    /// Current measured value.
    pub actual: RatchetCount,
    /// Current configured limit, when finite and applicable.
    pub limit: Option<RatchetCount>,
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
