//! Detection of ratchet entries without a corresponding current measurement.

use std::collections::{BTreeMap, BTreeSet};

use crate::check::report::{
    DirectoryRatchetStaleMissingViolation, DirectoryViolationKind,
    FileBytesRatchetStaleMissingViolation, FileRatchetStaleMissingViolation, FileViolationKind,
    RatchetStaleMissingViolation, RatchetViolation,
};
use crate::check::runner::SelectedPaths;
use crate::classification::{SourceKind, UnreadablePath, UnreadablePathKind};
use crate::ratchet::{DirectoryRatchetKey, FileRatchetKey, FileSizeRatchetKey, RatchetCount};
use crate::{DirectFiles, FileByteLimit, LineLimit, RelativePath};

pub(super) fn file_ratchets(
    ratchets: &BTreeMap<FileRatchetKey, LineLimit>,
    measured: &BTreeSet<FileRatchetKey>,
    scope: &JudgedScope<'_>,
) -> Vec<RatchetViolation> {
    missing_entries(ratchets, measured, scope, FileRatchetKey::path)
        .map(|(key, ceiling)| {
            RatchetViolation::Missing(RatchetStaleMissingViolation::File(
                FileRatchetStaleMissingViolation {
                    kind: file_kind(key.role()),
                    path: key.path().clone(),
                    ratchet: RatchetCount::from_loc_limit(*ceiling),
                },
            ))
        })
        .collect()
}

pub(super) fn directory_ratchets(
    ratchets: &BTreeMap<DirectoryRatchetKey, DirectFiles>,
    measured: &BTreeSet<DirectoryRatchetKey>,
    scope: &JudgedScope<'_>,
) -> Vec<RatchetViolation> {
    missing_entries(ratchets, measured, scope, DirectoryRatchetKey::path)
        .map(|(key, ceiling)| {
            RatchetViolation::Missing(RatchetStaleMissingViolation::Directory(
                DirectoryRatchetStaleMissingViolation {
                    kind: directory_kind(key.role()),
                    path: key.path().clone(),
                    group: key.group().clone(),
                    ratchet: RatchetCount::from_direct_files(*ceiling),
                },
            ))
        })
        .collect()
}

pub(super) fn file_size_ratchets(
    ratchets: &BTreeMap<FileSizeRatchetKey, FileByteLimit>,
    measured: &BTreeSet<FileSizeRatchetKey>,
    scope: &JudgedScope<'_>,
) -> Vec<RatchetViolation> {
    missing_entries(ratchets, measured, scope, FileSizeRatchetKey::path)
        .map(|(key, ceiling)| {
            RatchetViolation::Missing(RatchetStaleMissingViolation::FileBytes(
                FileBytesRatchetStaleMissingViolation {
                    path: key.path().clone(),
                    storage: key.storage(),
                    ratchet: *ceiling,
                },
            ))
        })
        .collect()
}

fn missing_entries<'a, Key: Ord, Ceiling>(
    ratchets: &'a BTreeMap<Key, Ceiling>,
    measured: &'a BTreeSet<Key>,
    scope: &'a JudgedScope<'a>,
    path: impl Fn(&Key) -> &RelativePath + 'a,
) -> impl Iterator<Item = (&'a Key, &'a Ceiling)> + 'a {
    ratchets
        .iter()
        .filter(move |(key, _)| scope.contains(path(key)) && !measured.contains(*key))
}

/// Whether a gap can hide a measurement of the paths it covers.
///
/// An unreadable `.gitattributes` leaves only the storage of files below it unknown, and a file
/// that fails to read leaves only its own measurement unknown. The check records the exact
/// entries each one leaves in doubt as measured, so neither gap hides anything else, such as the
/// ceiling for a storage the file has left.
const fn hides_measurements(gap: UnreadablePathKind) -> bool {
    match gap {
        UnreadablePathKind::ReadGitAttributesFailed | UnreadablePathKind::ReadFileFailed => false,
        UnreadablePathKind::ReadDirFailed
        | UnreadablePathKind::FileTypeFailed
        | UnreadablePathKind::SymlinkLoop
        | UnreadablePathKind::BrokenSymlink
        | UnreadablePathKind::IgnoreParseFailed => true,
    }
}

/// The paths where a missing measurement means the entry is stale: selected for the check and
/// outside every gap in the analysis that can hide a measurement.
///
/// A path inside an unreadable directory may still exist and still exceed its limit, so its
/// ratchet entry is kept. An unreadable path the traversal could not place covers the whole tree.
pub(in crate::check) struct JudgedScope<'a> {
    selected_paths: &'a SelectedPaths,
    gaps: &'a [UnreadablePath],
}

impl<'a> JudgedScope<'a> {
    pub(in crate::check) const fn new(
        selected_paths: &'a SelectedPaths,
        gaps: &'a [UnreadablePath],
    ) -> Self {
        Self {
            selected_paths,
            gaps,
        }
    }

    fn contains(&self, path: &RelativePath) -> bool {
        self.selected_paths.contains(path)
            && !self.gaps.iter().any(|gap| {
                hides_measurements(gap.kind)
                    && gap
                        .path
                        .as_ref()
                        .is_none_or(|gap_path| path.is_at_or_under(gap_path))
            })
    }
}

const fn file_kind(role: SourceKind) -> FileViolationKind {
    match role {
        SourceKind::Production => FileViolationKind::FileLines,
        SourceKind::Test => FileViolationKind::TestFileLines,
    }
}

const fn directory_kind(role: SourceKind) -> DirectoryViolationKind {
    match role {
        SourceKind::Production => DirectoryViolationKind::DirectoryDirectFiles,
        SourceKind::Test => DirectoryViolationKind::TestDirectoryDirectFiles,
    }
}
