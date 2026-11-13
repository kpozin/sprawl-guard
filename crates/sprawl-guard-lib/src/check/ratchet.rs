use std::collections::BTreeSet;
use std::path::Path;

use crate::config::Config;
use crate::ratchet::{
    DirectoryRatchetKey, FileRatchetKey, RatchetCount, RatchetFile, RatchetLookup,
};
use crate::{DirectFiles, LanguageGroupId, LineLimit, RelativePath, Result};

use super::directory_direct_files::{DirectoryDirectFilesCheck, DirectoryDirectFilesMeasurement};
use super::report::{CheckViolation, DirectoryViolationKind, FileViolationKind, RatchetEntryCount};
use super::report::{
    DirectoryRatchetStaleMeasuredViolation, FileRatchetStaleMeasuredViolation, RatchetLimit,
    RatchetStaleMeasuredViolation, RatchetViolation,
};
use super::runner::{FileMeasurement, SelectedPaths, SourceCheck};

mod missing;

pub(super) fn file_violations_with_ratchet(
    ratchet: &Option<RatchetState>,
    current_policy: RatchetCurrentPolicy,
    source_check: SourceCheck,
) -> Vec<CheckViolation> {
    let Some(ratchet) = ratchet else {
        return source_check
            .into_violation()
            .map(CheckViolation::FileLines)
            .into_iter()
            .collect();
    };
    let measurement = source_check.measurement();
    let ceiling = ratchet.file_ceiling(measurement.kind, &measurement.path);
    apply_file_ratchet(source_check, ceiling, current_policy)
}

fn apply_file_ratchet(
    mut source_check: SourceCheck,
    ceiling: Option<LineLimit>,
    current_policy: RatchetCurrentPolicy,
) -> Vec<CheckViolation> {
    let Some(ceiling) = ceiling else {
        return source_check
            .into_violation()
            .map(CheckViolation::FileLines)
            .into_iter()
            .collect();
    };
    let measurement = source_check.measurement();
    let actual = measurement.current_lines();
    if source_check.has_violation() && actual.get() > ceiling.get() {
        if let Some(violation) = source_check.violation_mut() {
            violation.ratchet = Some(ceiling);
        }
        return source_check
            .into_violation()
            .map(CheckViolation::FileLines)
            .into_iter()
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

fn stale_file_ratchet_violation(
    measurement: &FileMeasurement,
    ceiling: LineLimit,
) -> RatchetViolation {
    RatchetViolation::Measured(RatchetStaleMeasuredViolation::File(
        FileRatchetStaleMeasuredViolation {
            kind: measurement.kind,
            path: measurement.path.clone(),
            measurement: measurement.lines,
            limit: RatchetCount::from_loc_limit(measurement.limit),
            ratchet: RatchetCount::from_loc_limit(ceiling),
        },
    ))
}

pub(super) fn directory_violations_with_ratchet(
    ratchet: &Option<RatchetState>,
    current_policy: RatchetCurrentPolicy,
    directory_check: DirectoryDirectFilesCheck,
) -> Vec<CheckViolation> {
    let Some(ratchet) = ratchet else {
        return directory_check
            .into_violation()
            .map(CheckViolation::DirectoryDirectFiles)
            .into_iter()
            .collect();
    };
    let measurement = directory_check.measurement();
    let ceiling =
        ratchet.directory_ceiling(measurement.kind, &measurement.group, &measurement.path);
    apply_directory_ratchet(directory_check, ceiling, current_policy)
}

fn apply_directory_ratchet(
    mut directory_check: DirectoryDirectFilesCheck,
    ceiling: Option<DirectFiles>,
    current_policy: RatchetCurrentPolicy,
) -> Vec<CheckViolation> {
    let Some(ceiling) = ceiling else {
        return directory_check
            .into_violation()
            .map(CheckViolation::DirectoryDirectFiles)
            .into_iter()
            .collect();
    };
    let measurement = directory_check.measurement();
    let actual = measurement.actual;
    if directory_check.has_violation() && actual.get() > ceiling.get() {
        if let Some(violation) = directory_check.violation_mut() {
            violation.ratchet = Some(ceiling);
        }
        return directory_check
            .into_violation()
            .map(CheckViolation::DirectoryDirectFiles)
            .into_iter()
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

fn stale_directory_ratchet_violation(
    measurement: &DirectoryDirectFilesMeasurement,
    ceiling: DirectFiles,
) -> RatchetViolation {
    RatchetViolation::Measured(RatchetStaleMeasuredViolation::Directory(
        DirectoryRatchetStaleMeasuredViolation {
            kind: measurement.kind,
            path: measurement.path.clone(),
            group: measurement.group.clone(),
            actual: RatchetCount::from_direct_files(measurement.actual),
            limit: RatchetLimit::from(measurement.limit),
            ratchet: RatchetCount::from_direct_files(ceiling),
        },
    ))
}

pub(super) fn stale_missing_ratchets(
    ratchet: &Option<RatchetState>,
    selected_paths: &SelectedPaths,
    measured: &MeasuredRatchets,
) -> Vec<CheckViolation> {
    let Some(ratchet) = ratchet else {
        return Vec::new();
    };
    ratchet
        .missing_in(selected_paths, measured)
        .into_iter()
        .map(CheckViolation::Ratchet)
        .collect()
}

#[derive(Default)]
pub(super) struct MeasuredRatchets {
    files: BTreeSet<FileRatchetKey>,
    directories: BTreeSet<DirectoryRatchetKey>,
}

impl MeasuredRatchets {
    pub(super) fn record_file(&mut self, measurement: &FileMeasurement) {
        self.files.insert(FileRatchetKey::new(
            measurement.path.clone(),
            measurement.kind.role(),
        ));
    }

    pub(super) fn record_directory(&mut self, measurement: &DirectoryDirectFilesMeasurement) {
        self.directories.insert(DirectoryRatchetKey::new(
            measurement.path.clone(),
            measurement.kind.role(),
            measurement.group.clone(),
        ));
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
    ratchets: RatchetLookup,
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
        Ok(Some(Self::from_file(RatchetFile::read(&path)?)?))
    }

    /// Returns the number of file and directory ceilings in the loaded ratchet file.
    pub(super) fn entry_count(&self) -> RatchetEntryCount {
        RatchetEntryCount::from_len(self.ratchets.files().len() + self.ratchets.directories().len())
    }

    fn from_file(file: RatchetFile) -> Result<Self> {
        Ok(Self {
            ratchets: RatchetLookup::try_from(file)?,
        })
    }

    fn file_ceiling(&self, kind: FileViolationKind, path: &RelativePath) -> Option<LineLimit> {
        self.ratchets
            .file_ceiling(&FileRatchetKey::new(path.clone(), kind.role()))
    }

    fn directory_ceiling(
        &self,
        kind: DirectoryViolationKind,
        group: &LanguageGroupId,
        path: &RelativePath,
    ) -> Option<DirectFiles> {
        let key = DirectoryRatchetKey::new(path.clone(), kind.role(), group.clone());
        self.ratchets.directory_ceiling(&key)
    }

    fn missing_in(
        &self,
        selected_paths: &SelectedPaths,
        measured: &MeasuredRatchets,
    ) -> Vec<RatchetViolation> {
        let mut violations = Vec::new();
        violations.extend(missing::file_ratchets(
            self.ratchets.files(),
            &measured.files,
            selected_paths,
        ));
        violations.extend(missing::directory_ratchets(
            self.ratchets.directories(),
            &measured.directories,
            selected_paths,
        ));
        violations
    }
}
