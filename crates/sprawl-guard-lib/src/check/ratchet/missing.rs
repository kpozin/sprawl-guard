//! Detection of ratchet entries without a corresponding current measurement.

use std::collections::{BTreeMap, BTreeSet};

use crate::check::report::{
    DirectoryRatchetStaleMissingViolation, DirectoryViolationKind,
    FileRatchetStaleMissingViolation, FileViolationKind, RatchetStaleMissingViolation,
    RatchetStaleViolation, RatchetViolation,
};
use crate::check::runner::SelectedPaths;
use crate::classification::SourceKind;
use crate::ratchet::{DirectoryRatchetKey, FileRatchetKey, RatchetCount};
use crate::{DirectFiles, LineLimit, RelativePath};

pub(super) fn file_ratchets(
    ratchets: &BTreeMap<FileRatchetKey, LineLimit>,
    measured: &BTreeSet<FileRatchetKey>,
    selected_paths: &SelectedPaths,
) -> Vec<RatchetViolation> {
    missing_entries(ratchets, measured, selected_paths, FileRatchetKey::path)
        .map(|(key, ceiling)| {
            RatchetViolation::Stale(RatchetStaleViolation::Missing(
                RatchetStaleMissingViolation::File(FileRatchetStaleMissingViolation {
                    kind: file_kind(key.role()),
                    path: key.path().clone(),
                    ratchet: RatchetCount::from_loc_limit(*ceiling),
                }),
            ))
        })
        .collect()
}

pub(super) fn directory_ratchets(
    ratchets: &BTreeMap<DirectoryRatchetKey, DirectFiles>,
    measured: &BTreeSet<DirectoryRatchetKey>,
    selected_paths: &SelectedPaths,
) -> Vec<RatchetViolation> {
    missing_entries(
        ratchets,
        measured,
        selected_paths,
        DirectoryRatchetKey::path,
    )
    .map(|(key, ceiling)| {
        RatchetViolation::Stale(RatchetStaleViolation::Missing(
            RatchetStaleMissingViolation::Directory(DirectoryRatchetStaleMissingViolation {
                kind: directory_kind(key.role()),
                path: key.path().clone(),
                group: key.group().clone(),
                ratchet: RatchetCount::from_direct_files(*ceiling),
            }),
        ))
    })
    .collect()
}

fn missing_entries<'a, Key: Ord, Ceiling>(
    ratchets: &'a BTreeMap<Key, Ceiling>,
    measured: &'a BTreeSet<Key>,
    selected_paths: &'a SelectedPaths,
    path: impl Fn(&Key) -> &RelativePath + 'a,
) -> impl Iterator<Item = (&'a Key, &'a Ceiling)> + 'a {
    ratchets
        .iter()
        .filter(move |(key, _)| selected_paths.contains(path(key)) && !measured.contains(*key))
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
