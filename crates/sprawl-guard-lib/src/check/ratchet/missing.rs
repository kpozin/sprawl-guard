//! Detection of ratchet entries without a corresponding current measurement.

use std::collections::{BTreeMap, BTreeSet};

use crate::check::report::{
    DirectoryRatchetStaleMissingViolation, FileRatchetStaleMissingViolation,
    RatchetStaleMissingViolation, RatchetStaleViolation, RatchetStaleViolationKind,
    RatchetViolation,
};
use crate::check::runner::CheckScope;
use crate::ratchet::{DirectoryRatchetKey, RatchetCount};
use crate::{CodeLines, LeafFiles, RelativePath};

pub(super) fn file_ratchets(
    ratchets: &BTreeMap<RelativePath, CodeLines>,
    measured: &BTreeSet<RelativePath>,
    scope: &CheckScope,
    kind: RatchetStaleViolationKind,
) -> Vec<RatchetViolation> {
    matching_paths(
        ratchets,
        measured,
        scope,
        kind,
        RatchetCount::from_code_lines,
    )
}

pub(super) fn directory_ratchets(
    ratchets: &BTreeMap<DirectoryRatchetKey, LeafFiles>,
    measured: &BTreeSet<DirectoryRatchetKey>,
    scope: &CheckScope,
    kind: RatchetStaleViolationKind,
) -> Vec<RatchetViolation> {
    ratchets
        .iter()
        .filter(|(key, _)| scope.contains(key.path()) && !measured.contains(*key))
        .map(|(key, ceiling)| {
            RatchetViolation::Stale(RatchetStaleViolation::Missing(
                RatchetStaleMissingViolation::Directory(DirectoryRatchetStaleMissingViolation {
                    kind,
                    path: key.path().clone(),
                    group: key.group().clone(),
                    ratchet: RatchetCount::from_leaf_files(*ceiling),
                }),
            ))
        })
        .collect()
}

fn matching_paths<Ceiling>(
    ratchets: &BTreeMap<RelativePath, Ceiling>,
    measured: &BTreeSet<RelativePath>,
    scope: &CheckScope,
    kind: RatchetStaleViolationKind,
    ratchet_count: impl Fn(Ceiling) -> RatchetCount,
) -> Vec<RatchetViolation>
where
    Ceiling: Copy,
{
    ratchets
        .iter()
        .filter(|(path, _)| scope.contains(path) && !measured.contains(*path))
        .map(|(path, ceiling)| {
            RatchetViolation::Stale(RatchetStaleViolation::Missing(
                RatchetStaleMissingViolation::File(FileRatchetStaleMissingViolation {
                    kind,
                    path: path.clone(),
                    ratchet: ratchet_count(*ceiling),
                }),
            ))
        })
        .collect()
}
