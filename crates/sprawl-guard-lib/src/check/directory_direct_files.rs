use std::collections::BTreeMap;
use std::path::Path;

use crate::DirectFileLimit;
use crate::classification::{ClassifiedSource, SourceKind};
use crate::resolved_limits::LimitResolver;
use crate::{DirectFiles, LanguageGroupId, RelativePath, Result};

use super::{DirectoryDirectFilesViolation, DirectoryViolationKind};

pub(super) fn directory_direct_files_checks(
    limits: &LimitResolver<'_>,
    sources: &[ClassifiedSource],
) -> Result<Vec<DirectoryDirectFilesCheck>> {
    let mut directories = BTreeMap::<DirectoryKey, DirectoryCount>::new();
    for source in sources {
        let directory = source_parent_directory(&source.path)?;
        directories
            .entry(DirectoryKey {
                kind: source.kind,
                group: source.group().clone(),
                path: directory,
            })
            .or_default()
            .push(source.path.clone());
    }

    let mut checks = Vec::new();
    for (directory, count) in directories {
        let actual = count.actual();
        let kind = violation_kind(directory.kind);
        let limit =
            limits.directory_direct_files(directory.kind, &directory.group, &directory.path);
        let measurement = DirectoryDirectFilesMeasurement {
            kind,
            group: directory.group.clone(),
            path: directory.path.clone(),
            actual,
            limit: limit.value,
        };
        let check = match limit.value {
            DirectFileLimit::Finite(value) if actual.get() > value.get() => {
                DirectoryDirectFilesCheck::Exceeded(DirectoryDirectFilesViolation {
                    kind,
                    group: directory.group,
                    path: directory.path,
                    actual,
                    limit: value,
                    ratchet: None,
                    files: count.files.clone(),
                })
            }
            _ => DirectoryDirectFilesCheck::WithinLimit(measurement),
        };
        checks.push(check);
    }
    Ok(checks)
}

pub(super) enum DirectoryDirectFilesCheck {
    WithinLimit(DirectoryDirectFilesMeasurement),
    Exceeded(DirectoryDirectFilesViolation),
}

impl DirectoryDirectFilesCheck {
    pub(super) fn measurement(&self) -> DirectoryDirectFilesMeasurement {
        match self {
            Self::WithinLimit(measurement) => measurement.clone(),
            Self::Exceeded(violation) => DirectoryDirectFilesMeasurement {
                kind: violation.kind,
                group: violation.group.clone(),
                path: violation.path.clone(),
                actual: violation.actual,
                limit: DirectFileLimit::Finite(violation.limit),
            },
        }
    }

    pub(super) fn violation_mut(&mut self) -> Option<&mut DirectoryDirectFilesViolation> {
        match self {
            Self::WithinLimit(_) => None,
            Self::Exceeded(violation) => Some(violation),
        }
    }

    pub(super) fn has_violation(&self) -> bool {
        matches!(self, Self::Exceeded(_))
    }

    pub(super) fn into_violation(self) -> Option<DirectoryDirectFilesViolation> {
        match self {
            Self::WithinLimit(_) => None,
            Self::Exceeded(violation) => Some(violation),
        }
    }
}

#[derive(Clone)]
pub(super) struct DirectoryDirectFilesMeasurement {
    pub(super) kind: DirectoryViolationKind,
    pub(super) group: LanguageGroupId,
    pub(super) path: RelativePath,
    pub(super) actual: DirectFiles,
    pub(super) limit: DirectFileLimit,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DirectoryKey {
    kind: SourceKind,
    group: LanguageGroupId,
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

    fn actual(&self) -> DirectFiles {
        DirectFiles::trusted(
            self.files
                .len()
                .try_into()
                .expect("source file count must fit in u32"),
        )
    }
}

fn violation_kind(kind: SourceKind) -> DirectoryViolationKind {
    match kind {
        SourceKind::Production => DirectoryViolationKind::DirectoryDirectFiles,
        SourceKind::Test => DirectoryViolationKind::TestDirectoryDirectFiles,
    }
}

fn source_parent_directory(path: &RelativePath) -> Result<RelativePath> {
    let path = path.to_path_buf();
    RelativePath::new(path.parent().unwrap_or_else(|| Path::new("")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LanguageGroupId, LanguageId};

    fn production_source(path: &str, language: &str, group: LanguageGroupId) -> ClassifiedSource {
        ClassifiedSource::new(
            RelativePath::new(Path::new(path)).unwrap(),
            LanguageId::new(language).unwrap(),
            group,
            SourceKind::Production,
        )
    }

    mod when_languages_share_a_directory_but_not_a_group {
        use super::*;

        #[test]
        fn it_measures_each_group_independently() {
            let tempdir = tempfile::tempdir().unwrap();
            crate::test_support::write_config(
                tempdir.path(),
                indoc::indoc! {r#"
                    [limits.max_direct_files_per_directory]
                    production = 1

                    [language_groups.frontend]
                    languages = ["javascript", "typescript"]
                "#},
            );
            let config = crate::test_support::load_config(tempdir.path());
            let limits = LimitResolver::new(&config).unwrap();
            let frontend_group = LanguageGroupId::custom("frontend").unwrap();
            let sources = vec![
                production_source("src/app.js", "JavaScript", frontend_group.clone()),
                production_source("src/app.ts", "TypeScript", frontend_group.clone()),
                production_source("src/lib.rs", "Rust", LanguageGroupId::Source),
            ];

            let checks = directory_direct_files_checks(&limits, &sources).unwrap();
            let measurements = checks
                .iter()
                .map(DirectoryDirectFilesCheck::measurement)
                .collect::<Vec<_>>();

            assert_eq!(measurements.len(), 2);
            assert_eq!(measurements[0].group, LanguageGroupId::Source);
            assert_eq!(measurements[0].actual.get(), 1);
            assert_eq!(measurements[1].group, frontend_group);
            assert_eq!(measurements[1].actual.get(), 2);
        }
    }
}
