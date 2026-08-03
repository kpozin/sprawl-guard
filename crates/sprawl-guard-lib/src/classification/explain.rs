use std::io::ErrorKind;
use std::path::Path;

use crate::check::measure_source;
use crate::config::Config;
use crate::resolved_limits::LimitResolver;
use crate::rust_cfg::{
    RustCfgTestInfo, RustCfgTestStatus, preprocess_rust_cfg_test, rust_cfg_test_policy_info,
};
use crate::source_file::read_source_to_string;
use crate::value::path::{normalize_under_root, resolve_root};
use crate::{RelativePath, Result, SprawlError};

use super::classifier::Classifier;
use super::traversal::{is_source_file_candidate, is_walked_file};
use super::types::{ClassifiedSource, PathClassification, SourceExplanation};

/// Explains how source discovery classifies one path.
///
/// Relative paths are resolved under `root`. Existing source files keep the
/// requested in-repository identity for classification and reporting, including
/// paths reached through followed symlinked directories or a symlinked root.
/// The canonical root is still used to ask the traversal backend whether that
/// repository-relative path is visible to normal source discovery.
///
/// Missing paths and existing non-files may not canonicalize under that root.
/// In those cases, the fallback lexical root keeps diagnostics attached to the
/// requested path. Missing paths are still classified by pattern so users can
/// debug config before creating a file.
///
/// Existing non-files return `PathClassification::NonFile`. Existing files go
/// through ignore-aware traversal, language matching, and production/test
/// classification.
pub fn explain_path(root: &Path, path: &Path, config: &Config) -> Result<PathClassification> {
    let lexical_root = absolute_lexical_root(root)?;
    let root = resolve_root(root)?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    match path.symlink_metadata() {
        Ok(metadata) => {
            let file_type = metadata.file_type();
            if !is_source_file_candidate(&path, file_type, config)? {
                let path_for_relative = if metadata.file_type().is_dir() {
                    path.canonicalize().unwrap_or_else(|_| path.clone())
                } else {
                    path.clone()
                };
                let relative = normalize_explain_path(&root, &lexical_root, &path_for_relative)?;
                return Ok(PathClassification::NonFile { path: relative });
            }
            let relative = normalize_explain_path(&root, &lexical_root, &path)?;
            let walk_path = root.join(relative.to_path_buf());
            if !is_walked_file(&root, &walk_path, config)? {
                return Ok(PathClassification::Ignored { path: relative });
            }
            let classification = Classifier::new(config)?.classify(relative)?;
            return explain_existing_source(&path, config, classification);
        }
        Err(source) if source.kind() == ErrorKind::NotFound => {}
        Err(source) => {
            return Err(SprawlError::ReadFileType {
                path: path.to_path_buf(),
                source,
            });
        }
    }
    let relative = normalize_explain_path(&root, &lexical_root, &path)?;
    Classifier::new(config)?.classify(relative)
}

fn explain_existing_source(
    absolute_path: &Path,
    config: &Config,
    classification: PathClassification,
) -> Result<PathClassification> {
    let PathClassification::Source(explanation) = classification else {
        return Ok(classification);
    };
    let source = explanation.into_source();
    let limits = LimitResolver::new(config)?;
    let resolved_limits = limits.source_limits(&source)?;
    let measurement = measure_source(
        config,
        &source,
        absolute_path,
        resolved_limits.file.included_line_types,
    )?;
    let rust_cfg_test = rust_cfg_test_explanation(absolute_path, config, &source)?;
    Ok(PathClassification::Source(SourceExplanation::measured(
        source,
        resolved_limits,
        measurement,
        rust_cfg_test,
    )))
}

fn rust_cfg_test_explanation(
    absolute_path: &Path,
    config: &Config,
    source: &ClassifiedSource,
) -> Result<Option<RustCfgTestInfo>> {
    let Some(policy_info) = rust_cfg_test_policy_info(config, source) else {
        return Ok(None);
    };
    if policy_info.status != RustCfgTestStatus::PrefilterSkipped {
        return Ok(Some(policy_info));
    }

    let source_text = read_source_to_string(absolute_path, config.traversal.max_file_bytes)?;
    Ok(Some(preprocess_rust_cfg_test(&source_text).info))
}

fn absolute_lexical_root(root: &Path) -> Result<std::path::PathBuf> {
    std::path::absolute(root).map_err(|source| SprawlError::ResolveRoot {
        path: root.to_path_buf(),
        source,
    })
}

fn normalize_explain_path(
    canonical_root: &Path,
    lexical_root: &Path,
    path: &Path,
) -> Result<RelativePath> {
    match normalize_under_root(canonical_root, path) {
        Ok(path) => Ok(path),
        Err(SprawlError::PathOutsideRoot { .. }) if lexical_root.is_absolute() => {
            normalize_under_root(lexical_root, path)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use indoc::indoc;

    use super::*;
    use crate::classification::SourceExplanation;
    use crate::config::SymlinkTraversal;
    use crate::test_support::{
        load_config, write_ambiguous_rust_go_config, write_config, write_rust_enabled_config,
        write_rust_entry_limit_config, write_rust_symlink_traversal_config, write_source_file,
    };

    mod explain_path {
        use super::*;

        mod when_explaining_a_source_file {
            use super::*;

            fn explain_src_lib(root: &Path) -> PathClassification {
                explain_path(root, Path::new("src/lib.rs"), &load_config(root)).unwrap()
            }

            fn source_explanation(classification: PathClassification) -> SourceExplanation {
                let PathClassification::Source(explanation) = classification else {
                    panic!("expected source explanation");
                };
                explanation
            }

            #[test]
            fn it_reports_the_file_as_source() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                write_source_file(tempdir.path(), "src/lib.rs");

                let classification = explain_src_lib(tempdir.path());

                assert!(matches!(classification, PathClassification::Source(_)));
            }

            #[test]
            fn it_reports_rust_cfg_test_preprocessing_status() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                let path = tempdir.path().join("src/lib.rs");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(
                    &path,
                    indoc! {r#"
                        fn production() {}

                        #[cfg(test)]
                        mod tests {
                            fn fixture() {}
                        }
                    "#},
                )
                .unwrap();

                let explanation = source_explanation(explain_src_lib(tempdir.path()));
                let rust_cfg_test = explanation.rust_cfg_test().unwrap();
                assert_eq!(rust_cfg_test.status, RustCfgTestStatus::Parsed);
                assert_eq!(rust_cfg_test.excluded_items.get(), 1);
            }

            #[test]
            fn it_reports_prefilter_skips_for_rust_sources_without_cfg_test() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                write_source_file(tempdir.path(), "src/lib.rs");

                let explanation = source_explanation(explain_src_lib(tempdir.path()));
                let rust_cfg_test = explanation.rust_cfg_test().unwrap();
                assert_eq!(rust_cfg_test.status, RustCfgTestStatus::PrefilterSkipped);
                assert_eq!(rust_cfg_test.excluded_items.get(), 0);
            }

            #[test]
            fn it_rejects_sources_above_the_byte_size_limit_before_rust_cfg_explanation() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [traversal]
                        max_file_bytes = 8

                        [languages.rust]
                        enabled = true
                    "#},
                );
                write_source_file(tempdir.path(), "src/lib.rs");

                let error = explain_path(
                    tempdir.path(),
                    Path::new("src/lib.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap_err();

                assert!(matches!(
                    error,
                    SprawlError::SourceFileTooLarge { limit: 8, .. }
                ));
            }

            #[test]
            fn it_reports_resolved_limits_and_matching_overrides() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [limits]
                        max_lines_per_file = { production = 1 }
                        max_leaf_files_per_directory = { production = 1 }

                        [languages.rust]
                        enabled = true

                        [languages.rust.limits]
                        max_lines_per_file = { production = 2 }

                        [[overrides]]
                        paths = ["src/**"]
                        max_lines_per_file = { production = 3 }
                        max_leaf_files_per_directory = { production = 4 }
                        reason = "package exception"

                        [[overrides]]
                        paths = ["src/legacy/**"]
                        max_lines_per_file = { production = 5 }
                        reason = "legacy exception"
                    "#},
                );
                write_source_file(tempdir.path(), "src/legacy/lib.rs");

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("src/legacy/lib.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                let PathClassification::Source(explanation) = classification else {
                    panic!("expected source explanation");
                };
                let resolved_limits = explanation.resolved_limits().unwrap();
                assert_eq!(resolved_limits.file.value.get(), 5);
                assert_eq!(resolved_limits.directory.value.finite().unwrap().get(), 4);
                assert_eq!(resolved_limits.file.matched_overrides.len(), 2);
                assert_eq!(resolved_limits.file.matched_overrides[0].ordinal.get(), 1);
                assert_eq!(resolved_limits.file.matched_overrides[1].ordinal.get(), 2);
            }
        }

        mod when_explaining_a_globally_excluded_file {
            use super::*;

            #[test]
            fn it_reports_the_global_exclusion() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [files]
                        exclude = ["generated/**"]

                        [languages.rust]
                        enabled = true
                    "#},
                );
                write_source_file(tempdir.path(), "generated/lib.rs");

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("generated/lib.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(
                    classification,
                    PathClassification::GloballyExcluded { .. }
                ));
            }
        }

        mod when_explaining_a_language_excluded_file {
            use super::*;

            #[test]
            fn it_reports_the_language_exclusion() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [languages.rust]
                        enabled = true
                        exclude = ["src/generated.rs"]
                    "#},
                );
                write_source_file(tempdir.path(), "src/generated.rs");

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("src/generated.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(
                    classification,
                    PathClassification::LanguageExcluded { .. }
                ));
            }
        }

        mod when_explaining_a_file_excluded_by_multiple_languages {
            use super::*;

            #[test]
            fn it_reports_all_excluding_languages() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [languages.rust]
                        enabled = true
                        include = ["src/shared"]
                        exclude = ["src/shared"]

                        [languages.go]
                        enabled = true
                        include = ["src/shared"]
                        exclude = ["src/shared"]
                    "#},
                );
                write_source_file(tempdir.path(), "src/shared");

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("src/shared"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                let PathClassification::LanguagesExcluded { languages, .. } = classification else {
                    panic!("expected multi-language exclusion");
                };
                assert_eq!(languages.len(), 2);
            }
        }

        mod when_explaining_a_file_that_matches_multiple_languages {
            use super::*;

            #[test]
            fn it_reports_the_ambiguity() {
                let tempdir = tempfile::tempdir().unwrap();
                write_ambiguous_rust_go_config(tempdir.path(), "src/shared");
                write_source_file(tempdir.path(), "src/shared");

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("src/shared"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(
                    classification,
                    PathClassification::AmbiguousLanguage { .. }
                ));
            }
        }

        mod when_explaining_a_test_file_with_path_tests_disabled {
            use super::*;

            #[test]
            fn it_reports_the_test_exclusion() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [tests]
                        check_path_tests = false

                        [languages.rust]
                        enabled = true
                    "#},
                );
                write_source_file(tempdir.path(), "tests/integration.rs");

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("tests/integration.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(
                    classification,
                    PathClassification::TestExcluded { .. }
                ));
            }
        }

        mod when_explaining_a_gitignored_file {
            use super::*;

            #[test]
            fn it_reports_the_file_as_ignored() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                fs::write(tempdir.path().join(".gitignore"), "ignored.rs\n").unwrap();
                write_source_file(tempdir.path(), "ignored.rs");

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("ignored.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(classification, PathClassification::Ignored { .. }));
            }
        }

        mod when_the_traversal_entry_limit_is_reached {
            use super::*;

            #[test]
            fn it_reports_a_traversal_limit_error() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_entry_limit_config(tempdir.path(), 1);
                write_source_file(tempdir.path(), "src/lib.rs");

                let error = explain_path(
                    tempdir.path(),
                    Path::new("src/lib.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap_err();

                assert!(matches!(
                    error,
                    SprawlError::TraversalEntryLimitExceeded { limit: 1, .. }
                ));
            }
        }

        mod when_explaining_a_hidden_file {
            use super::*;

            #[test]
            fn it_reports_the_file_as_ignored_by_default() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                write_source_file(tempdir.path(), ".hidden/lib.rs");

                let classification = explain_path(
                    tempdir.path(),
                    Path::new(".hidden/lib.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(classification, PathClassification::Ignored { .. }));
            }
        }

        mod when_explaining_a_directory_with_a_source_name {
            use super::*;

            #[test]
            fn it_reports_the_directory_as_a_non_file() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                fs::create_dir_all(tempdir.path().join("src/lib.rs")).unwrap();

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("src/lib.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(classification, PathClassification::NonFile { .. }));
            }
        }

        #[cfg(unix)]
        mod when_explaining_a_file_under_a_symlinked_directory {
            use super::*;

            #[test]
            fn it_reports_the_symlink_path() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [traversal]
                        follow_symlinks = "directories"

                        [languages.rust]
                        enabled = true
                    "#},
                );
                write_source_file(tempdir.path(), "real/lib.rs");
                std::os::unix::fs::symlink("real", tempdir.path().join("linked")).unwrap();

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("linked/lib.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                let PathClassification::Source(explanation) = classification else {
                    panic!("expected source explanation");
                };
                assert_eq!(explanation.source().path.as_str(), "linked/lib.rs");
            }
        }

        #[cfg(unix)]
        mod when_explaining_a_broken_symlink_with_a_source_name {
            use super::*;

            #[test]
            fn it_reports_the_symlink_as_a_non_file() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                std::os::unix::fs::symlink("missing.rs", tempdir.path().join("broken.rs")).unwrap();

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("broken.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(classification, PathClassification::NonFile { .. }));
            }
        }

        #[cfg(unix)]
        mod when_explaining_a_symlink_to_a_file_outside_the_root {
            use super::*;

            #[test]
            fn it_reports_the_symlink_as_a_non_file() {
                let tempdir = tempfile::tempdir().unwrap();
                let external = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                write_source_file(external.path(), "outside.rs");
                fs::create_dir_all(tempdir.path().join("src")).unwrap();
                std::os::unix::fs::symlink(
                    external.path().join("outside.rs"),
                    tempdir.path().join("src/link.rs"),
                )
                .unwrap();

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("src/link.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                assert!(matches!(classification, PathClassification::NonFile { .. }));
            }
        }

        #[cfg(unix)]
        mod when_the_root_is_a_symlink {
            use super::*;

            struct SymlinkedRoot {
                _parent: tempfile::TempDir,
                real_root: PathBuf,
                link_root: PathBuf,
            }

            fn create_symlinked_root() -> SymlinkedRoot {
                create_symlinked_root_in(tempfile::tempdir().unwrap())
            }

            fn create_symlinked_root_in(parent: tempfile::TempDir) -> SymlinkedRoot {
                let real_root = parent.path().join("real");
                let link_root = parent.path().join("link");
                fs::create_dir(&real_root).unwrap();
                std::os::unix::fs::symlink(&real_root, &link_root).unwrap();
                SymlinkedRoot {
                    _parent: parent,
                    real_root,
                    link_root,
                }
            }

            fn write_external_file_symlink(real_root: &Path, external: &Path) {
                write_source_file(external, "outside.rs");
                fs::create_dir_all(real_root.join("src")).unwrap();
                std::os::unix::fs::symlink(
                    external.join("outside.rs"),
                    real_root.join("src/link.rs"),
                )
                .unwrap();
            }

            #[test]
            fn it_accepts_absolute_paths_through_the_symlinked_root() {
                let root = create_symlinked_root();
                write_rust_enabled_config(&root.real_root);
                write_source_file(&root.real_root, "src/lib.rs");

                let classification = explain_path(
                    &root.link_root,
                    &root.link_root.join("src/lib.rs"),
                    &load_config(&root.real_root),
                )
                .unwrap();

                assert!(matches!(classification, PathClassification::Source(_)));
            }

            #[test]
            fn it_reports_absolute_file_symlink_paths_through_the_symlinked_root() {
                let root = create_symlinked_root();
                write_rust_symlink_traversal_config(&root.real_root, SymlinkTraversal::Files);
                write_source_file(&root.real_root, "src/target.rs");
                std::os::unix::fs::symlink("target.rs", root.real_root.join("src/link.rs"))
                    .unwrap();

                let classification = explain_path(
                    &root.link_root,
                    &root.link_root.join("src/link.rs"),
                    &load_config(&root.real_root),
                )
                .unwrap();

                let PathClassification::Source(explanation) = classification else {
                    panic!("expected source explanation");
                };
                assert_eq!(explanation.source().path.as_str(), "src/link.rs");
            }

            #[test]
            fn it_reports_absolute_symlink_non_files_through_the_symlinked_root() {
                let external = tempfile::tempdir().unwrap();
                let root = create_symlinked_root();
                write_rust_enabled_config(&root.real_root);
                write_external_file_symlink(&root.real_root, external.path());

                let classification = explain_path(
                    &root.link_root,
                    &root.link_root.join("src/link.rs"),
                    &load_config(&root.real_root),
                )
                .unwrap();

                assert!(matches!(classification, PathClassification::NonFile { .. }));
            }

            #[test]
            fn it_reports_absolute_symlink_non_files_through_a_relative_symlinked_root() {
                let current_dir = std::env::current_dir().unwrap();
                let external = tempfile::tempdir().unwrap();
                let root = create_symlinked_root_in(tempfile::tempdir_in(&current_dir).unwrap());
                let relative_link_root = root.link_root.strip_prefix(&current_dir).unwrap();
                write_rust_enabled_config(&root.real_root);
                write_external_file_symlink(&root.real_root, external.path());

                let classification = explain_path(
                    relative_link_root,
                    &root.link_root.join("src/link.rs"),
                    &load_config(&root.real_root),
                )
                .unwrap();

                assert!(matches!(classification, PathClassification::NonFile { .. }));
            }
        }
    }
}
