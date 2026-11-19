//! Detection of ratchet entries without a corresponding current measurement.

use std::collections::{BTreeMap, BTreeSet};

use crate::check::report::{
    DirectoryRatchetStaleMissingViolation, DirectoryViolationKind,
    FileBytesRatchetStaleMissingViolation, FileRatchetStaleMissingViolation, FileViolationKind,
    RatchetStaleMissingViolation, RatchetViolation,
};
use crate::check::runner::SelectedPaths;
use crate::classification::{SourceKind, UnreadablePath};
use crate::ratchet::{DirectoryRatchetKey, FileRatchetKey, RatchetCount};
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
    ratchets: &BTreeMap<RelativePath, FileByteLimit>,
    measured: &BTreeSet<RelativePath>,
    scope: &JudgedScope<'_>,
) -> Vec<RatchetViolation> {
    missing_entries(ratchets, measured, scope, |path| path)
        .map(|(path, ceiling)| {
            RatchetViolation::Missing(RatchetStaleMissingViolation::FileBytes(
                FileBytesRatchetStaleMissingViolation {
                    path: path.clone(),
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

/// The paths where a missing measurement means the entry is stale: selected for the check and
/// outside every gap in the analysis.
///
/// A path inside an unreadable directory, or one whose own read failed, may still exist and
/// still exceed its limit, so its ratchet entry is kept. An unreadable path the traversal could
/// not place covers the whole tree.
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
                gap.path
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
