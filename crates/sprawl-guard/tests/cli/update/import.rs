use assert_fs::{TempDir, prelude::*};

use super::*;

mod when_importing_eslint_suppressions {
    use super::*;

    #[test]
    fn it_does_not_require_eslint_without_an_over_limit_file() {
        let root = TempDir::new().unwrap();
        root.child("short.js").write_str("const x = 1;").unwrap();
        let output = sprawl_guard(&root)
            .args(["update", "--import", "eslint"])
            .output()
            .unwrap();
        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout(&output).contains("No ratchet changes"));
    }

    #[test]
    fn it_rejects_paths_outside_the_project_before_looking_for_eslint() {
        let outer = TempDir::new().unwrap();
        outer.child("project").create_dir_all().unwrap();
        outer
            .child("neighbor.js")
            .write_str("const x = 1;")
            .unwrap();
        let output = sprawl_guard(&outer)
            .current_dir(outer.path().join("project"))
            .args(["update", "--import", "eslint", "../neighbor.js"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(stderr(&output).contains(".."), "{}", stderr(&output));
        assert!(!stderr(&output).contains("ESLint"), "{}", stderr(&output));
    }
}

mod when_importing_oxlint_suppressions {
    use super::*;
    use sprawl_guard_lib::RatchetFile;
    use sprawl_guard_lib::classification::SourceKind;

    const OVER_LIMIT: &str = "const a = 1;\nconst b = 2;\n";

    fn import(root: &TempDir) -> RatchetFile {
        let output = sprawl_guard(root)
            .args(["update", "--import", "oxlint", "--write"])
            .output()
            .unwrap();
        assert_exit_code(&output, exit_codes::SUCCESS);
        RatchetFile::read(&root.path().join("sprawl-guard.ratchet.toml")).unwrap()
    }

    mod and_the_file_has_a_whole_file_disable {
        use super::*;

        #[test]
        fn it_records_the_current_test_file_line_count() {
            let root = TempDir::new().unwrap();
            write_config(&root, "[limits.max_lines_per_file]\ntest = 1\n");
            for (path, header) in [
                ("src/all.test.ts", "/* oxlint-disable */"),
                (
                    "src/inline.test.ts",
                    "/* oxlint-disable max-lines -- legacy */",
                ),
            ] {
                root.child(path)
                    .write_str(&format!("{header}\n{OVER_LIMIT}"))
                    .unwrap();
            }

            let ratchet = import(&root);
            let entries = ratchet
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.role, file.max_lines.get()))
                .collect::<Vec<_>>();
            assert_eq!(
                entries,
                vec![
                    ("src/all.test.ts", SourceKind::Test, 2),
                    ("src/inline.test.ts", SourceKind::Test, 2),
                ]
            );
        }
    }

    mod and_oxlint_is_not_installed {
        use super::*;

        #[test]
        fn it_skips_eslint_style_directives() {
            let root = TempDir::new().unwrap();
            write_config(&root, "[limits.max_lines_per_file]\ntest = 1\n");
            root.child("src/legacy.test.ts")
                .write_str(&format!("/* eslint-disable max-lines */\n{OVER_LIMIT}"))
                .unwrap();

            assert!(import(&root).files.is_empty());
        }
    }

    mod and_the_file_has_a_bulk_suppression {
        use super::*;

        #[test]
        fn it_preserves_the_existing_ceiling_and_ignores_other_rules_and_metrics() {
            let root = TempDir::new().unwrap();
            write_config(
                &root,
                "[limits.max_lines_per_file]\nproduction = 1\n\n[limits.max_direct_files_per_directory]\nproduction = 1\n",
            );
            root.child("oxlint-suppressions.json")
                .write_str(r#"{"src/bulk.js":{"max-lines":{"count":1}},"src/other.js":{"no-debugger":{"count":1}}}"#)
                .unwrap();
            root.child("src/bulk.js").write_str(OVER_LIMIT).unwrap();
            root.child("src/other.js").write_str(OVER_LIMIT).unwrap();
            root.child("sprawl-guard.ratchet.toml")
                .write_str(
                    "schema_version = 1\n[[files]]\npath = \"src/bulk.js\"\nrole = \"production\"\nmax_lines = 5\n",
                )
                .unwrap();

            let ratchet = import(&root);
            let [file] = ratchet.files.as_slice() else {
                panic!("expected only the bulk entry")
            };
            assert_eq!(file.path.as_str(), "src/bulk.js");
            assert_eq!(file.max_lines.get(), 5);
            assert!(ratchet.directories.is_empty());
        }
    }
}
