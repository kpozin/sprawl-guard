use std::collections::BTreeSet;
use std::path::Path;

use crate::classification::SourceKind;
use crate::config::Config;
use crate::ratchet::{
    DirectoryRatchetKey, FileRatchetKey, FileSizeRatchetKey, RatchetCount, RatchetFile,
    RatchetLookup,
};
use crate::{
    DirectFiles, FileByteLimit, FileStorage, LanguageGroupId, LineLimit, RelativePath, Result,
};

use super::directory_direct_files::{DirectoryDirectFilesCheck, DirectoryDirectFilesMeasurement};
use super::report::{CheckViolation, FileViolationKind, RatchetEntryCount};
use super::report::{
    DirectoryRatchetStaleMeasuredViolation, FileRatchetStaleMeasuredViolation, RatchetLimit,
    RatchetStaleMeasuredViolation, RatchetViolation,
};
use super::runner::{FileMeasurement, SourceCheck};

mod missing;

pub(super) use missing::JudgedScope;

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
    let ceiling = ratchet.directory_ceiling(
        measurement.kind.role(),
        &measurement.group,
        &measurement.path,
    );
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
    scope: &JudgedScope<'_>,
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
    files: BTreeSet<FileRatchetKey>,
    directories: BTreeSet<DirectoryRatchetKey>,
    file_sizes: BTreeSet<FileSizeRatchetKey>,
}

impl MeasuredRatchets {
    pub(super) fn record_file(&mut self, measurement: &FileMeasurement) {
        self.files.insert(FileRatchetKey::new(
            measurement.path.clone(),
            measurement.kind.role(),
        ));
    }

    /// Records a file whose lines were not counted, because it was too large or failed to read,
    /// so its line ceiling is not reported missing.
    pub(super) fn record_uncounted_file(&mut self, path: &RelativePath, kind: FileViolationKind) {
        self.files
            .insert(FileRatchetKey::new(path.clone(), kind.role()));
    }

    /// Records a file whose size was measured, or failed to read, so its byte ceiling is not
    /// reported missing.
    pub(super) fn record_file_size(&mut self, key: FileSizeRatchetKey) {
        self.file_sizes.insert(key);
    }

    /// Records a file whose storage is unknown, because a `.gitattributes` file above it can't be
    /// read, so neither of its byte ceilings is reported missing.
    pub(super) fn record_unknown_storage_file(&mut self, path: &RelativePath) {
        self.file_sizes
            .extend(FileStorage::ALL.map(|storage| FileSizeRatchetKey::new(path.clone(), storage)));
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

/// Ceilings from the ratchet file, indexed for lookup by measured path.
pub(crate) struct RatchetState {
    ratchets: RatchetLookup,
}

impl RatchetState {
    /// Reads the configured ratchet file, or returns `None` when the file does not exist.
    pub(crate) fn load_if_present(root: &Path, config: &Config) -> Result<Option<Self>> {
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
        RatchetEntryCount::from_len(
            self.ratchets.files().len()
                + self.ratchets.directories().len()
                + self.ratchets.file_sizes().len(),
        )
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

    /// Returns the byte ceiling for one file.
    pub(super) fn file_size_ceiling(&self, key: &FileSizeRatchetKey) -> Option<FileByteLimit> {
        self.ratchets.file_size_ceiling(key)
    }

    /// Returns the direct-file ceiling for one directory's production or test files in `group`.
    pub(crate) fn directory_ceiling(
        &self,
        role: SourceKind,
        group: &LanguageGroupId,
        path: &RelativePath,
    ) -> Option<DirectFiles> {
        let key = DirectoryRatchetKey::new(path.clone(), role, group.clone());
        self.ratchets.directory_ceiling(&key)
    }

    fn missing_in(
        &self,
        scope: &JudgedScope<'_>,
        measured: &MeasuredRatchets,
    ) -> Vec<RatchetViolation> {
        let mut violations = Vec::new();
        violations.extend(missing::file_ratchets(
            self.ratchets.files(),
            &measured.files,
            scope,
        ));
        violations.extend(missing::directory_ratchets(
            self.ratchets.directories(),
            &measured.directories,
            scope,
        ));
        violations.extend(missing::file_size_ratchets(
            self.ratchets.file_sizes(),
            &measured.file_sizes,
            scope,
        ));
        violations
    }
}
