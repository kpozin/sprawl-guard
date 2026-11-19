use std::io::ErrorKind;
use std::path::Path;

use crate::check::measure_source;
use crate::config::Config;
use crate::resolved_limits::LimitResolver;
use crate::value::path::{normalize_under_root, resolve_root};
use crate::{RelativePath, Result, SprawlError};

use super::classifier::Classifier;
use super::explain_directory::explain_directory;
use super::traversal::{is_source_file_candidate, is_walked_directory, is_walked_file};
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
/// Existing directories that the walk reaches return `PathClassification::Directory`
/// with the direct file counts that `check` measures there; directories it skips
/// return `PathClassification::Ignored`. Other existing non-files return
/// `PathClassification::NonFile`. Existing files go through ignore-aware
/// traversal, language matching, and production/test classification.
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
            let followed_directory_link = file_type.is_symlink()
                && config.traversal.follow_symlinks.follows_directories()
                && path.is_dir();
            if file_type.is_dir() || followed_directory_link {
                let relative = normalize_explain_path(&root, &lexical_root, &path)?;
                if !is_walked_directory(&root, &root.join(relative.to_path_buf()), config)? {
                    return Ok(PathClassification::Ignored { path: relative });
                }
                return explain_directory(&root, relative, config)
                    .map(PathClassification::Directory);
            }
            if !is_source_file_candidate(&path, file_type, config)? {
                let relative = normalize_explain_path(&root, &lexical_root, &path)?;
                return Ok(PathClassification::NonFile { path: relative });
            }
            let relative = normalize_explain_path(&root, &lexical_root, &path)?;
            let walk_path = root.join(relative.to_path_buf());
            if !is_walked_file(&root, &walk_path, config)? {
                return Ok(PathClassification::Ignored { path: relative });
            }
            return Classifier::new(config)?
                .classify(relative)?
                .try_map_source(|source| measure_existing_source(&path, config, source));
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
    Classifier::new(config)?
        .classify(relative)?
        .try_map_source(|source| Ok(SourceExplanation::not_found(source)))
}

#[cfg(test)]
#[path = "explain_scoped_tests.rs"]
mod scoped_tests;

/// Resolves the limits for an existing source file and measures it.
fn measure_existing_source(
    absolute_path: &Path,
    config: &Config,
    source: ClassifiedSource,
) -> Result<SourceExplanation> {
    let limits = LimitResolver::new(config)?;
    let resolved_limits = limits.source_limits(&source)?;
    let measurement = measure_source(
        config,
        &source,
        absolute_path,
        resolved_limits.file.count_lines,
    )?;
    Ok(SourceExplanation::measured(
        source,
        resolved_limits,
        measurement.measurement,
        measurement.rust_cfg_test,
    ))
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
    use crate::rust_cfg::RustCfgTestStatus;
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
                        max_parsed_code_file_bytes = 8

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
                    SprawlError::ParsedCodeFileTooLarge { limit, .. } if limit.into_inner() == 8
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
                        max_direct_files_per_directory = { production = 1 }

                        [languages.rust]
                        enabled = true

                        [languages.rust.limits]
                        max_lines_per_file = { production = 2 }

                        [[overrides]]
                        paths = ["src/**"]
                        max_lines_per_file = { production = 3 }
                        max_direct_files_per_directory = { production = 4 }
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
                        extend_exclude = ["generated/**"]

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
                        check = false

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
            fn it_reports_the_directory() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                fs::create_dir_all(tempdir.path().join("src/lib.rs")).unwrap();

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("src/lib.rs"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                let PathClassification::Directory(directory) = classification else {
                    panic!("expected directory explanation");
                };
                assert_eq!(directory.path.as_str(), "src/lib.rs");
            }
        }

        mod when_explaining_a_directory_the_walk_skips {
            use super::*;

            fn explain_directory_path(root: &Path, path: &str) -> PathClassification {
                explain_path(root, Path::new(path), &load_config(root)).unwrap()
            }

            #[test]
            fn it_reports_a_gitignored_directory_as_ignored() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                fs::write(tempdir.path().join(".gitignore"), "generated/\n").unwrap();
                write_source_file(tempdir.path(), "generated/lib.rs");

                let classification = explain_directory_path(tempdir.path(), "generated");

                assert!(matches!(classification, PathClassification::Ignored { .. }));
            }

            #[test]
            fn it_reports_a_hidden_directory_as_ignored() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                write_source_file(tempdir.path(), ".hidden/lib.rs");

                let classification = explain_directory_path(tempdir.path(), ".hidden");

                assert!(matches!(classification, PathClassification::Ignored { .. }));
            }
        }

        mod when_the_directory_has_a_malformed_ignore_file {
            use super::*;

            #[test]
            fn it_measures_the_directory_and_reports_the_ignore_file() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                write_source_file(tempdir.path(), "src/lib.rs");
                fs::write(tempdir.path().join("src/.gitignore"), "{a\n").unwrap();

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("src"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                let PathClassification::Directory(directory) = classification else {
                    panic!("expected directory explanation");
                };
                assert_eq!(directory.measurements[0].files[0].as_str(), "src/lib.rs");
                assert!(!directory.unreadable_paths.is_empty());
            }
        }

        mod when_explaining_the_root_directory {
            use super::*;

            #[test]
            fn it_counts_the_files_directly_in_the_root() {
                let tempdir = tempfile::tempdir().unwrap();
                write_rust_enabled_config(tempdir.path());
                write_source_file(tempdir.path(), "build.rs");
                write_source_file(tempdir.path(), "src/lib.rs");

                let classification =
                    explain_path(tempdir.path(), Path::new("."), &load_config(tempdir.path()))
                        .unwrap();

                let PathClassification::Directory(directory) = classification else {
                    panic!("expected directory explanation");
                };
                let [measurement] = directory.measurements.as_slice() else {
                    panic!("expected one measurement");
                };
                assert_eq!(measurement.files[0].as_str(), "build.rs");
            }
        }

        #[cfg(unix)]
        mod when_explaining_a_symlinked_directory {
            use super::*;

            fn explain_linked(symlinks: &str) -> PathClassification {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    &format!(
                        "[traversal]\nfollow_symlinks = \"{symlinks}\"\n\n\
                         [languages.rust]\nenabled = true\n"
                    ),
                );
                write_source_file(tempdir.path(), "real/lib.rs");
                std::os::unix::fs::symlink("real", tempdir.path().join("linked")).unwrap();
                explain_path(
                    tempdir.path(),
                    Path::new("linked"),
                    &load_config(tempdir.path()),
                )
                .unwrap()
            }

            #[test]
            fn it_counts_files_under_the_link_name_when_directory_links_are_followed() {
                let PathClassification::Directory(directory) = explain_linked("directories") else {
                    panic!("expected directory explanation");
                };
                assert_eq!(directory.path.as_str(), "linked");
                assert_eq!(directory.measurements[0].files[0].as_str(), "linked/lib.rs");
            }

            #[test]
            fn it_keeps_the_link_name_for_a_directory_under_the_link() {
                let tempdir = tempfile::tempdir().unwrap();
                write_config(
                    tempdir.path(),
                    indoc! {r#"
                        [traversal]
                        follow_symlinks = "directories"

                        [languages.rust]
                        enabled = true

                        [[overrides]]
                        paths = ["linked/sub"]
                        max_direct_files_per_directory = { production = 1 }
                        reason = "alias only"
                    "#},
                );
                write_source_file(tempdir.path(), "real/sub/lib.rs");
                std::os::unix::fs::symlink("real", tempdir.path().join("linked")).unwrap();

                let classification = explain_path(
                    tempdir.path(),
                    Path::new("linked/sub"),
                    &load_config(tempdir.path()),
                )
                .unwrap();

                let PathClassification::Directory(directory) = classification else {
                    panic!("expected directory explanation");
                };
                assert_eq!(directory.path.as_str(), "linked/sub");
                assert_eq!(directory.measurements[0].limit.matched_overrides.len(), 1);
            }

            #[test]
            fn it_reports_a_non_file_when_directory_links_are_not_followed() {
                assert!(matches!(
                    explain_linked("files"),
                    PathClassification::NonFile { .. }
                ));
            }
        }

        #[cfg(unix)]
        mod when_explaining_a_directory_through_a_symlinked_root {
            use super::*;

            #[test]
            fn it_counts_the_files_directly_inside_it() {
                let parent = tempfile::tempdir().unwrap();
                let real_root = parent.path().join("real");
                let link_root = parent.path().join("link");
                fs::create_dir(&real_root).unwrap();
                write_rust_enabled_config(&real_root);
                write_source_file(&real_root, "src/lib.rs");
                std::os::unix::fs::symlink(&real_root, &link_root).unwrap();

                let classification =
                    explain_path(&link_root, &link_root.join("src"), &load_config(&real_root))
                        .unwrap();

                let PathClassification::Directory(directory) = classification else {
                    panic!("expected directory explanation");
                };
                assert_eq!(directory.measurements[0].files[0].as_str(), "src/lib.rs");
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
