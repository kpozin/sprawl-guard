//! Explains how `check` measures the source files directly inside one directory.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::check::RatchetState;
use crate::config::{Config, UnreadablePathPolicy};
use crate::resolved_limits::LimitResolver;
use crate::{DirectFiles, LanguageGroupId, RelativePath, ResolvedDirectFileLimit, Result};

use super::source_discovery::discover_sources_report;
use super::types::{SourceKind, UnreadablePath};

/// Direct-file measurements for one existing directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DirectoryExplanation {
    /// Repository-relative path.
    pub path: RelativePath,
    /// One measurement per production or test kind and language group with files directly here.
    ///
    /// The list is empty when no included source file sits directly in the directory.
    pub measurements: Vec<DirectFilesExplanation>,
    /// Paths at or under the directory that could not be read, so the measurements may be
    /// incomplete.
    pub unreadable_paths: Vec<UnreadablePath>,
}

/// How `check` measures one kind and language group of files directly in a directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DirectFilesExplanation {
    /// Production or test kind of the counted files.
    pub kind: SourceKind,
    /// Language group of the counted files.
    pub group: LanguageGroupId,
    /// Included source files directly in the directory, in path order.
    pub files: Vec<RelativePath>,
    /// Resolved direct-file limit and the overrides that set it.
    pub limit: ResolvedDirectFileLimit,
    /// Ceiling from the ratchet file, when it has one for this directory, kind, and group.
    pub ratchet: Option<DirectFiles>,
}

/// Measures the included source files directly inside `path`, which must be an existing directory.
///
/// Discovery walks only `path` and its descendants, in path order, then keeps the files whose
/// parent is `path`,
/// so the files and counts match what `check` reports for the same directory. Unreadable
/// descendants are reported as warnings whatever `[traversal].unreadable_paths` says, because an
/// explanation is diagnostic and should still show what could be measured.
pub(super) fn explain_directory(
    root: &Path,
    path: RelativePath,
    config: &Config,
) -> Result<DirectoryExplanation> {
    let discovery = discover_sources_report(
        root,
        config,
        std::slice::from_ref(&path),
        Some(UnreadablePathPolicy::Warn),
    )?;
    let limits = LimitResolver::new(config)?;
    let ratchet = RatchetState::load_if_present(root, config)?;
    let directory = path.to_path_buf();
    let mut buckets = BTreeMap::<(SourceKind, LanguageGroupId), Vec<RelativePath>>::new();
    for source in discovery.sources {
        if source.path.to_path_buf().parent() == Some(directory.as_path()) {
            buckets
                .entry((source.kind, source.group().clone()))
                .or_default()
                .push(source.path);
        }
    }
    let measurements = buckets
        .into_iter()
        .map(|((kind, group), files)| DirectFilesExplanation {
            limit: limits.directory_direct_files(kind, &group, &path),
            ratchet: ratchet
                .as_ref()
                .and_then(|ratchet| ratchet.directory_ceiling(kind, &group, &path)),
            kind,
            group,
            files,
        })
        .collect();
    Ok(DirectoryExplanation {
        path,
        measurements,
        unreadable_paths: discovery.unreadable_paths,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use indoc::indoc;

    use super::*;
    use crate::test_support::{load_config, write_config, write_source_file};

    fn explain(root: &Path, path: &str) -> DirectoryExplanation {
        explain_directory(
            root,
            RelativePath::new(Path::new(path)).unwrap(),
            &load_config(root),
        )
        .unwrap()
    }

    fn file_names(measurement: &DirectFilesExplanation) -> Vec<&str> {
        measurement.files.iter().map(RelativePath::as_str).collect()
    }

    mod explain_directory {
        use super::*;

        mod when_the_directory_has_production_and_test_files {
            use super::*;

            #[test]
            fn it_measures_each_kind_separately_and_skips_nested_files() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(tempdir.path(), "[languages.rust]\nenabled = true\n");
                write_source_file(tempdir.path(), "src/b.rs");
                write_source_file(tempdir.path(), "src/a.rs");
                write_source_file(tempdir.path(), "src/a_test.rs");
                write_source_file(tempdir.path(), "src/nested/c.rs");

                let explanation = explain(tempdir.path(), "src");

                let kinds = explanation
                    .measurements
                    .iter()
                    .map(|measurement| (measurement.kind, file_names(measurement)))
                    .collect::<Vec<_>>();
                assert_eq!(
                    kinds,
                    [
                        (SourceKind::Production, vec!["src/a.rs", "src/b.rs"]),
                        (SourceKind::Test, vec!["src/a_test.rs"]),
                    ]
                );
            }
        }

        mod when_an_override_and_a_ratchet_apply {
            use super::*;

            #[test]
            fn it_reports_the_override_and_the_ceiling() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [languages.rust]
                        enabled = true

                        [[overrides]]
                        paths = ["src"]
                        max_direct_files_per_directory = { production = 1 }
                        reason = "flat module"
                    "#},
                );
                fs::write(
                    tempdir.path().join("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[directories]]
                        path = "src"
                        role = "production"
                        group = "source"
                        max_direct_files = 2
                    "#},
                )
                .unwrap();
                write_source_file(tempdir.path(), "src/a.rs");
                write_source_file(tempdir.path(), "src/b.rs");

                let explanation = explain(tempdir.path(), "src");

                let [measurement] = explanation.measurements.as_slice() else {
                    panic!("expected one measurement");
                };
                assert_eq!(measurement.group, LanguageGroupId::Source);
                assert_eq!(measurement.limit.value.finite().unwrap().get(), 1);
                assert_eq!(measurement.limit.matched_overrides.len(), 1);
                assert_eq!(measurement.ratchet.unwrap().get(), 2);
            }
        }

        mod when_the_directory_has_no_direct_sources {
            use super::*;

            #[test]
            fn it_reports_no_measurements() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(tempdir.path(), "[languages.rust]\nenabled = true\n");
                write_source_file(tempdir.path(), "src/nested/lib.rs");

                let explanation = explain(tempdir.path(), "src");

                assert_eq!(explanation.path.as_str(), "src");
                assert!(explanation.measurements.is_empty());
            }
        }

        #[cfg(unix)]
        mod when_a_descendant_cannot_be_read {
            use super::*;
            use crate::classification::{UnreadablePathKind, UnreadablePathSeverity};

            #[test]
            fn it_reports_the_path_as_a_warning_even_when_config_fails_on_them() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [traversal]
                        follow_symlinks = "directories"
                        unreadable_paths = "fail"

                        [languages.rust]
                        enabled = true
                    "#},
                );
                write_source_file(tempdir.path(), "src/lib.rs");
                std::os::unix::fs::symlink("..", tempdir.path().join("src/cycle")).unwrap();

                let explanation = explain(tempdir.path(), "src");

                assert_eq!(file_names(&explanation.measurements[0]), ["src/lib.rs"]);
                let [unreadable] = explanation.unreadable_paths.as_slice() else {
                    panic!("expected one unreadable path");
                };
                assert_eq!(unreadable.kind, UnreadablePathKind::SymlinkLoop);
                assert_eq!(unreadable.severity, UnreadablePathSeverity::Warning);
            }
        }
    }
}
