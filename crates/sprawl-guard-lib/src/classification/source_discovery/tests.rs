#![cfg(test)]

use std::fs;
use std::path::Path;

use indoc::indoc;

use super::{discover_sources, discover_sources_report};
use crate::classification::{SourceKind, UnreadablePathKind, UnreadablePathSeverity};
use crate::config::SymlinkTraversal;
use crate::test_support::{
    assert_symlink_loop_error, load_config, source_paths, write_config, write_rust_enabled_config,
    write_rust_entry_limit_config, write_rust_symlink_traversal_config,
    write_rust_symlink_traversal_warn_config, write_source_file,
};

mod discover_sources {
    use super::*;

    mod when_skipping_hidden_names {
        use super::*;

        #[test]
        fn it_prunes_dot_names_but_keeps_regular_names() {
            let root = tempfile::tempdir().unwrap();
            write_rust_enabled_config(root.path());
            write_source_file(root.path(), ".private/lib.rs");
            write_source_file(root.path(), "src/.hidden.rs");
            write_source_file(root.path(), "src/visible.rs");

            let sources = discover_sources(root.path(), &load_config(root.path())).unwrap();

            assert_eq!(source_paths(&sources), ["src/visible.rs"]);
        }
    }

    fn write_ignore_fixture(root: &Path, ignore_file_name: &str) {
        fs::write(root.join(ignore_file_name), "ignored.rs\n").unwrap();
        write_source_file(root, "ignored.rs");
        write_source_file(root, "included.rs");
    }

    #[cfg(unix)]
    fn write_file_symlink_fixture(root: &Path) {
        use std::os::unix::fs::symlink;

        write_source_file(root, "target.rs");
        symlink("target.rs", root.join("link.rs")).unwrap();
    }

    #[cfg(unix)]
    fn write_directory_symlink_fixture(root: &Path) {
        use std::os::unix::fs::symlink;

        write_source_file(root, "real/lib.rs");
        symlink("real", root.join("linked")).unwrap();
    }

    mod when_assigning_automatic_language_groups {
        use super::*;
        use crate::LanguageGroupId;

        #[test]
        fn it_assigns_literate_languages_to_documentation() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.markdown]
                    enabled = true
                "#},
            );
            write_source_file(tempdir.path(), "README.md");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].group(), &LanguageGroupId::Documentation);
        }

        #[test]
        fn it_assigns_non_literate_languages_to_source() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_source_file(tempdir.path(), "src/lib.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].group(), &LanguageGroupId::Source);
        }

        #[test]
        fn it_uses_the_resolved_languages_group_for_a_custom_extension() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.markdown]
                    extend_include = ["**/*.guide"]
                "#},
            );
            write_source_file(tempdir.path(), "docs/start.guide");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].group(), &LanguageGroupId::Documentation);
        }
    }

    mod when_gitignore_excludes_a_file {
        use super::*;

        #[test]
        fn it_does_not_discover_the_file() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_ignore_fixture(tempdir.path(), ".gitignore");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["included.rs"]);
        }
    }

    mod when_ignore_file_excludes_a_file {
        use super::*;

        #[test]
        fn it_does_not_discover_the_file() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_ignore_fixture(tempdir.path(), ".ignore");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["included.rs"]);
        }
    }

    mod when_rgignore_excludes_a_file {
        use super::*;

        #[test]
        fn it_does_not_discover_the_file() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_ignore_fixture(tempdir.path(), ".rgignore");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["included.rs"]);
        }
    }

    mod when_fdignore_excludes_a_file {
        use super::*;

        #[test]
        fn it_does_not_discover_the_file() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_ignore_fixture(tempdir.path(), ".fdignore");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["included.rs"]);
        }
    }

    mod when_a_hidden_directory_contains_sources {
        use super::*;

        #[test]
        fn it_skips_the_directory_by_default() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_source_file(tempdir.path(), ".hidden/lib.rs");
            write_source_file(tempdir.path(), "src/lib.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["src/lib.rs"]);
        }

        #[test]
        fn it_includes_the_directory_when_hidden_paths_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [ignore]
                    skip_hidden = false

                    [languages.rust]
                    enabled = true
                "#},
            );
            write_source_file(tempdir.path(), ".hidden/lib.rs");
            write_source_file(tempdir.path(), "src/lib.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec![".hidden/lib.rs", "src/lib.rs"]);
        }
    }

    #[cfg(unix)]
    mod when_symlinked_files_exist {
        use std::os::unix::fs::symlink;

        use super::*;

        #[test]
        fn it_skips_them_by_default() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_file_symlink_fixture(tempdir.path());

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["target.rs"]);
        }

        #[test]
        fn it_includes_them_when_file_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(tempdir.path(), SymlinkTraversal::Files);
            write_file_symlink_fixture(tempdir.path());

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["link.rs", "target.rs"]);
        }

        #[test]
        fn it_skips_them_when_only_directory_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(tempdir.path(), SymlinkTraversal::Directories);
            write_file_symlink_fixture(tempdir.path());

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["target.rs"]);
        }

        #[test]
        fn it_includes_them_when_all_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(
                tempdir.path(),
                SymlinkTraversal::FilesAndDirectories,
            );
            write_file_symlink_fixture(tempdir.path());

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["link.rs", "target.rs"]);
        }

        #[test]
        fn it_skips_broken_symlinks_when_file_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(tempdir.path(), SymlinkTraversal::Files);
            symlink("missing.rs", tempdir.path().join("broken.rs")).unwrap();
            write_source_file(tempdir.path(), "src/lib.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["src/lib.rs"]);
        }
    }

    #[cfg(unix)]
    mod when_symlinked_directories_exist {
        use std::os::unix::fs::symlink;

        use super::*;

        #[test]
        fn it_skips_them_by_default() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_directory_symlink_fixture(tempdir.path());

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["real/lib.rs"]);
        }

        #[test]
        fn it_descends_through_them_when_directory_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(tempdir.path(), SymlinkTraversal::Directories);
            write_directory_symlink_fixture(tempdir.path());

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["linked/lib.rs", "real/lib.rs"]);
        }

        #[test]
        fn it_skips_them_when_only_file_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(tempdir.path(), SymlinkTraversal::Files);
            write_directory_symlink_fixture(tempdir.path());

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["real/lib.rs"]);
        }

        #[test]
        fn it_descends_through_them_when_all_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(
                tempdir.path(),
                SymlinkTraversal::FilesAndDirectories,
            );
            write_directory_symlink_fixture(tempdir.path());

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["linked/lib.rs", "real/lib.rs"]);
        }

        #[test]
        fn it_reports_collected_unreadable_paths_for_cycles_when_directory_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(tempdir.path(), SymlinkTraversal::Directories);
            write_source_file(tempdir.path(), "real/lib.rs");
            symlink("..", tempdir.path().join("real/cycle")).unwrap();

            let error = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap_err();

            assert_symlink_loop_error(error);
        }

        #[test]
        fn it_returns_warnings_when_unreadable_paths_is_warn() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_warn_config(tempdir.path(), SymlinkTraversal::Directories);
            write_source_file(tempdir.path(), "real/lib.rs");
            symlink("..", tempdir.path().join("real/cycle")).unwrap();

            let report = discover_sources_report(
                tempdir.path(),
                &load_config(tempdir.path()),
                &[],
                None,
                None,
            )
            .unwrap();

            assert_eq!(source_paths(&report.sources), vec!["real/lib.rs"]);
            assert_eq!(report.unreadable_paths.len(), 1);
            assert_eq!(
                report.unreadable_paths[0].kind,
                UnreadablePathKind::SymlinkLoop
            );
            assert_eq!(
                report.unreadable_paths[0].severity,
                UnreadablePathSeverity::Warning
            );
        }
    }

    mod when_max_entries_is_reached {
        use super::*;

        #[test]
        fn it_reports_a_traversal_limit_error() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_entry_limit_config(tempdir.path(), 1);
            write_source_file(tempdir.path(), "src/lib.rs");

            let error = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap_err();

            assert!(matches!(
                error,
                crate::SprawlError::TraversalEntryLimitExceeded { limit: 1, .. }
            ));
        }
    }

    #[cfg(unix)]
    mod when_special_files_exist {
        use std::os::unix::net::UnixListener;
        use std::process::Command;

        use super::*;

        #[test]
        fn it_skips_unix_sockets_even_when_the_name_matches_a_source_glob() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            let socket_path = tempdir.path().join("socket.rs");
            let _listener = UnixListener::bind(&socket_path).unwrap();
            write_source_file(tempdir.path(), "src/lib.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["src/lib.rs"]);
        }

        #[test]
        fn it_skips_fifos_even_when_the_name_matches_a_source_glob() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            let fifo_path = tempdir.path().join("pipe.rs");
            let status = Command::new("mkfifo").arg(&fifo_path).status().unwrap();
            assert!(status.success());
            write_source_file(tempdir.path(), "src/lib.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["src/lib.rs"]);
        }

        #[test]
        fn it_skips_device_nodes_even_when_the_name_matches_a_source_glob() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            let device_path = tempdir.path().join("device.rs");
            let output = Command::new("mknod")
                .arg(&device_path)
                .arg("c")
                .arg("1")
                .arg("3")
                .output();
            let Ok(output) = output else {
                return;
            };
            if !output.status.success() {
                return;
            }
            write_source_file(tempdir.path(), "src/lib.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["src/lib.rs"]);
        }
    }

    #[cfg(windows)]
    mod when_windows_filesystem_edges_exist {
        use std::path::Path;
        use std::process::Command;

        use super::*;

        #[test]
        fn it_discovers_sources_under_long_paths() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            let long_component = "nested".repeat(40);
            let source_path = format!("{long_component}/{long_component}/lib.rs");
            write_source_file(tempdir.path(), &source_path);

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec![source_path.as_str()]);
        }

        #[test]
        fn it_descends_through_directory_junctions_when_directory_symlinks_are_enabled() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_symlink_traversal_config(tempdir.path(), SymlinkTraversal::Directories);
            write_source_file(tempdir.path(), "real/lib.rs");
            create_junction(&tempdir.path().join("linked"), &tempdir.path().join("real"));

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["linked/lib.rs", "real/lib.rs"]);
        }

        fn create_junction(link: &Path, target: &Path) {
            let output = Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "failed to create junction: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    mod when_global_excludes_match {
        use super::*;

        #[test]
        fn it_excludes_matching_sources() {
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
            write_source_file(tempdir.path(), "src/lib.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["src/lib.rs"]);
        }
    }

    mod when_test_globs_match {
        use super::*;

        #[test]
        fn it_classifies_global_test_matches_as_tests() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_source_file(tempdir.path(), "tests/integration.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(sources[0].kind, SourceKind::Test);
        }

        #[test]
        fn it_classifies_language_specific_test_matches_as_tests() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_enabled_config(tempdir.path());
            write_source_file(tempdir.path(), "benches/bench.rs");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(sources[0].kind, SourceKind::Test);
        }

        #[test]
        fn it_excludes_test_matches_when_path_tests_are_disabled() {
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

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert!(sources.is_empty());
        }
    }
}
