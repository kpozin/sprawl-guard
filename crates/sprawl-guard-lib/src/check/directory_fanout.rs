use std::collections::BTreeMap;
use std::path::Path;

use crate::classification::{ClassifiedSource, SourceKind};
use crate::resolved_limits::LimitResolver;
use crate::{LeafFiles, RelativePath, Result};

use super::{DirectoryFanoutViolation, DirectoryViolationKind};

pub(super) fn directory_fanout_checks(
    limits: &LimitResolver<'_>,
    sources: &[ClassifiedSource],
) -> Result<Vec<DirectoryFanoutCheck>> {
    let mut directories = BTreeMap::<DirectoryKey, DirectoryCount>::new();
    for source in sources {
        let directory = source_parent_directory(&source.path)?;
        directories
            .entry(DirectoryKey {
                kind: source.kind,
                path: directory,
            })
            .or_default()
            .push(source.path.clone());
    }

    let mut checks = Vec::new();
    for (directory, count) in directories {
        let actual = count.actual();
        let kind = violation_kind(directory.kind);
        let limit = limits.directory_leaf_files(directory.kind, &directory.path);
        let measurement = DirectoryFanoutMeasurement {
            kind,
            path: directory.path.clone(),
            actual,
            limit: limit.as_ref().map(|limit| limit.value),
        };
        let check = match limit {
            Some(limit) if actual.get() > limit.value.get() => {
                DirectoryFanoutCheck::Exceeded(DirectoryFanoutViolation {
                    kind,
                    path: directory.path,
                    actual,
                    limit: limit.value,
                    ratchet: None,
                    files: count.files.clone(),
                })
            }
            _ => DirectoryFanoutCheck::WithinLimit(measurement),
        };
        checks.push(check);
    }
    Ok(checks)
}

pub(super) enum DirectoryFanoutCheck {
    WithinLimit(DirectoryFanoutMeasurement),
    Exceeded(DirectoryFanoutViolation),
}

impl DirectoryFanoutCheck {
    pub(super) fn measurement(&self) -> DirectoryFanoutMeasurement {
        match self {
            Self::WithinLimit(measurement) => measurement.clone(),
            Self::Exceeded(violation) => DirectoryFanoutMeasurement {
                kind: violation.kind,
                path: violation.path.clone(),
                actual: violation.actual,
                limit: Some(violation.limit),
            },
        }
    }

    pub(super) fn violation_mut(&mut self) -> Option<&mut DirectoryFanoutViolation> {
        match self {
            Self::WithinLimit(_) => None,
            Self::Exceeded(violation) => Some(violation),
        }
    }

    pub(super) fn has_violation(&self) -> bool {
        matches!(self, Self::Exceeded(_))
    }

    pub(super) fn into_violation(self) -> Option<DirectoryFanoutViolation> {
        match self {
            Self::WithinLimit(_) => None,
            Self::Exceeded(violation) => Some(violation),
        }
    }
}

#[derive(Clone)]
pub(super) struct DirectoryFanoutMeasurement {
    pub(super) kind: DirectoryViolationKind,
    pub(super) path: RelativePath,
    pub(super) actual: LeafFiles,
    pub(super) limit: Option<LeafFiles>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DirectoryKey {
    kind: SourceKind,
    path: RelativePath,
}

#[derive(Debug, Default)]
struct DirectoryCount {
    files: Vec<RelativePath>,
}

impl DirectoryCount {
    fn push(&mut self, file: RelativePath) {
        self.files.push(file);
    }

    fn actual(&self) -> LeafFiles {
        LeafFiles::trusted(
            self.files
                .len()
                .try_into()
                .expect("source file count must fit in u32"),
        )
    }
}

fn violation_kind(kind: SourceKind) -> DirectoryViolationKind {
    match kind {
        SourceKind::Production => DirectoryViolationKind::DirectoryFanout,
        SourceKind::Test => DirectoryViolationKind::TestDirectoryFanout,
    }
}

fn source_parent_directory(path: &RelativePath) -> Result<RelativePath> {
    let path = path.to_path_buf();
    RelativePath::new(path.parent().unwrap_or_else(|| Path::new("")))
}
