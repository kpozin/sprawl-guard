use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::config::Config;
use crate::ratchet::{RatchetCount, RatchetFile};
use crate::{CodeLines, LeafFiles, RelativePath, Result};

use super::directory_fanout::{DirectoryFanoutCheck, DirectoryFanoutMeasurement};
use super::report::{
    CheckViolation, DirectoryViolationKind, FileViolationKind, RatchetIncreasedViolation,
    RatchetIncreasedViolationKind, RatchetStaleMeasuredViolation, RatchetStaleMissingViolation,
    RatchetStaleViolation, RatchetStaleViolationKind, RatchetViolation,
};
use super::runner::{CheckScope, FileMeasurement, SourceCheck};

pub(super) fn file_violations_with_ratchet(
    ratchet: &Option<RatchetState>,
    current_policy: RatchetCurrentPolicy,
    source_check: SourceCheck,
) -> Vec<CheckViolation> {
    let Some(ratchet) = ratchet else {
        return source_check
            .into_violation()
            .map(CheckViolation::FileLoc)
            .into_iter()
            .collect();
    };
    let measurement = source_check.measurement();
    let ceiling = ratchet.file_ceiling(measurement.kind, &measurement.path);
    apply_file_ratchet(source_check, ceiling, current_policy)
}

fn apply_file_ratchet(
    mut source_check: SourceCheck,
    ceiling: Option<CodeLines>,
    current_policy: RatchetCurrentPolicy,
) -> Vec<CheckViolation> {
    let Some(ceiling) = ceiling else {
        return source_check
            .into_violation()
            .map(CheckViolation::FileLoc)
            .into_iter()
            .collect();
    };
    let measurement = source_check.measurement();
    let actual = measurement.current_code_lines();
    if source_check.has_violation() && actual.get() > ceiling.get() {
        if let Some(violation) = source_check.violation_mut() {
            violation.ratchet = Some(ceiling);
        }
        let ratchet_violation = increased_file_ratchet_violation(&measurement, ceiling);
        return source_check
            .into_violation()
            .map(CheckViolation::FileLoc)
            .into_iter()
            .chain(std::iter::once(CheckViolation::Ratchet(ratchet_violation)))
            .collect();
    }
    if current_policy.requires_current()
        && (actual.get() < ceiling.get() || !source_check.has_violation())
    {
        return vec![CheckViolation::Ratchet(stale_file_ratchet_violation(
            &measurement,
            ceiling,
        ))];
    }
    Vec::new()
}

fn increased_file_ratchet_violation(
    measurement: &FileMeasurement,
    ceiling: CodeLines,
) -> RatchetViolation {
    RatchetViolation::Increased(RatchetIncreasedViolation {
        kind: file_ratchet_increased_kind(measurement.kind),
        path: measurement.path.clone(),
        actual: RatchetCount::from_line_count(measurement.current_code_lines()),
        limit: RatchetCount::from_code_lines(measurement.limit),
        ratchet: RatchetCount::from_code_lines(ceiling),
    })
}

fn stale_file_ratchet_violation(
    measurement: &FileMeasurement,
    ceiling: CodeLines,
) -> RatchetViolation {
    RatchetViolation::Stale(RatchetStaleViolation::Measured(
        RatchetStaleMeasuredViolation {
            kind: file_ratchet_stale_kind(measurement.kind),
            path: measurement.path.clone(),
            actual: RatchetCount::from_line_count(measurement.current_code_lines()),
            limit: Some(RatchetCount::from_code_lines(measurement.limit)),
            ratchet: RatchetCount::from_code_lines(ceiling),
        },
    ))
}

fn file_ratchet_increased_kind(kind: FileViolationKind) -> RatchetIncreasedViolationKind {
    match kind {
        FileViolationKind::FileLoc => RatchetIncreasedViolationKind::FileLocIncreased,
        FileViolationKind::TestFileLoc => RatchetIncreasedViolationKind::TestFileLocIncreased,
    }
}

fn file_ratchet_stale_kind(kind: FileViolationKind) -> RatchetStaleViolationKind {
    match kind {
        FileViolationKind::FileLoc => RatchetStaleViolationKind::FileLocStale,
        FileViolationKind::TestFileLoc => RatchetStaleViolationKind::TestFileLocStale,
    }
}

pub(super) fn directory_violations_with_ratchet(
    ratchet: &Option<RatchetState>,
    current_policy: RatchetCurrentPolicy,
    directory_check: DirectoryFanoutCheck,
) -> Vec<CheckViolation> {
    let Some(ratchet) = ratchet else {
        return directory_check
            .into_violation()
            .map(CheckViolation::DirectoryFanout)
            .into_iter()
            .collect();
    };
    let measurement = directory_check.measurement();
    let ceiling = ratchet.directory_ceiling(measurement.kind, &measurement.path);
    apply_directory_ratchet(directory_check, ceiling, current_policy)
}

fn apply_directory_ratchet(
    mut directory_check: DirectoryFanoutCheck,
    ceiling: Option<LeafFiles>,
    current_policy: RatchetCurrentPolicy,
) -> Vec<CheckViolation> {
    let Some(ceiling) = ceiling else {
        return directory_check
            .into_violation()
            .map(CheckViolation::DirectoryFanout)
            .into_iter()
            .collect();
    };
    let measurement = directory_check.measurement();
    let actual = measurement.actual;
    if directory_check.has_violation() && actual.get() > ceiling.get() {
        if let Some(violation) = directory_check.violation_mut() {
            violation.ratchet = Some(ceiling);
        }
        let ratchet_violation = increased_directory_ratchet_violation(&measurement, ceiling);
        return directory_check
            .into_violation()
            .map(CheckViolation::DirectoryFanout)
            .into_iter()
            .chain(std::iter::once(CheckViolation::Ratchet(ratchet_violation)))
            .collect();
    }
    if current_policy.requires_current()
        && (actual.get() < ceiling.get() || !directory_check.has_violation())
    {
        return vec![CheckViolation::Ratchet(stale_directory_ratchet_violation(
            &measurement,
            ceiling,
        ))];
    }
    Vec::new()
}

fn increased_directory_ratchet_violation(
    measurement: &DirectoryFanoutMeasurement,
    ceiling: LeafFiles,
) -> RatchetViolation {
    RatchetViolation::Increased(RatchetIncreasedViolation {
        kind: directory_ratchet_increased_kind(measurement.kind),
        path: measurement.path.clone(),
        actual: RatchetCount::from_leaf_files(measurement.actual),
        limit: RatchetCount::from_leaf_files(
            measurement
                .limit
                .expect("increased directory ratchet must have a finite configured limit"),
        ),
        ratchet: RatchetCount::from_leaf_files(ceiling),
    })
}

fn stale_directory_ratchet_violation(
    measurement: &DirectoryFanoutMeasurement,
    ceiling: LeafFiles,
) -> RatchetViolation {
    RatchetViolation::Stale(RatchetStaleViolation::Measured(
        RatchetStaleMeasuredViolation {
            kind: directory_ratchet_stale_kind(measurement.kind),
            path: measurement.path.clone(),
            actual: RatchetCount::from_leaf_files(measurement.actual),
            limit: measurement.limit.map(RatchetCount::from_leaf_files),
            ratchet: RatchetCount::from_leaf_files(ceiling),
        },
    ))
}

fn directory_ratchet_increased_kind(kind: DirectoryViolationKind) -> RatchetIncreasedViolationKind {
    match kind {
        DirectoryViolationKind::DirectoryFanout => {
            RatchetIncreasedViolationKind::DirectoryFanoutIncreased
        }
        DirectoryViolationKind::TestDirectoryFanout => {
            RatchetIncreasedViolationKind::TestDirectoryFanoutIncreased
        }
    }
}

fn directory_ratchet_stale_kind(kind: DirectoryViolationKind) -> RatchetStaleViolationKind {
    match kind {
        DirectoryViolationKind::DirectoryFanout => RatchetStaleViolationKind::DirectoryFanoutStale,
        DirectoryViolationKind::TestDirectoryFanout => {
            RatchetStaleViolationKind::TestDirectoryFanoutStale
        }
    }
}

pub(super) fn stale_missing_ratchets(
    ratchet: &Option<RatchetState>,
    scope: &CheckScope,
    measured: &MeasuredRatchets,
) -> Vec<CheckViolation> {
    let Some(ratchet) = ratchet else {
        return Vec::new();
    };
    ratchet
        .missing_in(scope, measured)
        .into_iter()
        .map(CheckViolation::Ratchet)
        .collect()
}

#[derive(Default)]
pub(super) struct MeasuredRatchets {
    file_loc: BTreeSet<RelativePath>,
    test_file_loc: BTreeSet<RelativePath>,
    directory_fanout: BTreeSet<RelativePath>,
    test_directory_fanout: BTreeSet<RelativePath>,
}

impl MeasuredRatchets {
    pub(super) fn record_file(&mut self, measurement: &FileMeasurement) {
        let paths = match measurement.kind {
            FileViolationKind::FileLoc => &mut self.file_loc,
            FileViolationKind::TestFileLoc => &mut self.test_file_loc,
        };
        paths.insert(measurement.path.clone());
    }

    pub(super) fn record_directory(&mut self, measurement: &DirectoryFanoutMeasurement) {
        let paths = match measurement.kind {
            DirectoryViolationKind::DirectoryFanout => &mut self.directory_fanout,
            DirectoryViolationKind::TestDirectoryFanout => &mut self.test_directory_fanout,
        };
        paths.insert(measurement.path.clone());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RatchetCurrentPolicy {
    RequireCurrent,
    AllowStale,
}

impl RatchetCurrentPolicy {
    pub(super) const fn requires_current(self) -> bool {
        matches!(self, Self::RequireCurrent)
    }
}

pub(super) struct RatchetState {
    file_loc: BTreeMap<RelativePath, CodeLines>,
    test_file_loc: BTreeMap<RelativePath, CodeLines>,
    directory_fanout: BTreeMap<RelativePath, LeafFiles>,
    test_directory_fanout: BTreeMap<RelativePath, LeafFiles>,
}

impl RatchetState {
    pub(super) fn load_if_present(root: &Path, config: &Config) -> Result<Option<Self>> {
        let path = root.join(config.ratchet.file.as_str());
        if !path
            .try_exists()
            .map_err(|source| crate::SprawlError::ReadFileType {
                path: path.clone(),
                source,
            })?
        {
            return Ok(None);
        }
        Ok(Some(Self::from_file(RatchetFile::read(&path)?)))
    }

    fn from_file(file: RatchetFile) -> Self {
        Self {
            file_loc: file
                .files
                .into_iter()
                .map(|entry| (entry.path, entry.non_test_code_lines))
                .collect(),
            test_file_loc: file
                .test_files
                .into_iter()
                .map(|entry| (entry.path, entry.code_lines))
                .collect(),
            directory_fanout: file
                .directories
                .into_iter()
                .map(|entry| (entry.path, entry.leaf_source_files))
                .collect(),
            test_directory_fanout: file
                .test_directories
                .into_iter()
                .map(|entry| (entry.path, entry.leaf_test_files))
                .collect(),
        }
    }

    fn file_ceiling(&self, kind: FileViolationKind, path: &RelativePath) -> Option<CodeLines> {
        match kind {
            FileViolationKind::FileLoc => self.file_loc.get(path),
            FileViolationKind::TestFileLoc => self.test_file_loc.get(path),
        }
        .copied()
    }

    fn directory_ceiling(
        &self,
        kind: DirectoryViolationKind,
        path: &RelativePath,
    ) -> Option<LeafFiles> {
        match kind {
            DirectoryViolationKind::DirectoryFanout => self.directory_fanout.get(path),
            DirectoryViolationKind::TestDirectoryFanout => self.test_directory_fanout.get(path),
        }
        .copied()
    }

    fn missing_in(&self, scope: &CheckScope, measured: &MeasuredRatchets) -> Vec<RatchetViolation> {
        let mut violations = Vec::new();
        violations.extend(missing_file_ratchets(
            &self.file_loc,
            &measured.file_loc,
            scope,
            RatchetStaleViolationKind::FileLocStale,
        ));
        violations.extend(missing_file_ratchets(
            &self.test_file_loc,
            &measured.test_file_loc,
            scope,
            RatchetStaleViolationKind::TestFileLocStale,
        ));
        violations.extend(missing_directory_ratchets(
            &self.directory_fanout,
            &measured.directory_fanout,
            scope,
            RatchetStaleViolationKind::DirectoryFanoutStale,
        ));
        violations.extend(missing_directory_ratchets(
            &self.test_directory_fanout,
            &measured.test_directory_fanout,
            scope,
            RatchetStaleViolationKind::TestDirectoryFanoutStale,
        ));
        violations
    }
}

fn missing_file_ratchets(
    ratchets: &BTreeMap<RelativePath, CodeLines>,
    measured: &BTreeSet<RelativePath>,
    scope: &CheckScope,
    kind: RatchetStaleViolationKind,
) -> Vec<RatchetViolation> {
    missing_ratchets(
        ratchets,
        measured,
        scope,
        kind,
        RatchetCount::from_code_lines,
    )
}

fn missing_directory_ratchets(
    ratchets: &BTreeMap<RelativePath, LeafFiles>,
    measured: &BTreeSet<RelativePath>,
    scope: &CheckScope,
    kind: RatchetStaleViolationKind,
) -> Vec<RatchetViolation> {
    missing_ratchets(
        ratchets,
        measured,
        scope,
        kind,
        RatchetCount::from_leaf_files,
    )
}

fn missing_ratchets<Ceiling>(
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
                RatchetStaleMissingViolation {
                    kind,
                    path: path.clone(),
                    ratchet: ratchet_count(*ceiling),
                },
            ))
        })
        .collect()
}
