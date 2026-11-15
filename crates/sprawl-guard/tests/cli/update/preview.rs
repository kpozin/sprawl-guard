use assert_fs::TempDir;
use indoc::indoc;

use super::super::common::*;

#[cfg(unix)]
mod when_a_path_contains_terminal_controls {
    use super::*;

    #[test]
    fn it_keeps_each_file_and_directory_change_on_one_escaped_line() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            "[limits.max_lines_per_file]\nproduction = 1\n[limits.max_direct_files_per_directory]\nproduction = 1\n",
        );
        let directory = "src\n! raise\r\u{1b}[2J";
        write_rust_source_with_function_count(&root, &format!("{directory}/a.rs"), 2);
        write_rust_source_with_function_count(&root, &format!("{directory}/b.rs"), 2);

        let output = sprawl_guard(&root).arg("update").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        let summary = stdout(&output);
        assert_eq!(summary.lines().count(), 6, "{summary:?}");
        assert!(!summary.contains('\r'));
        assert!(!summary.contains('\u{1b}'));
        let escaped = r"src\n! raise\r\u{1b}[2J";
        assert!(summary.contains(&format!("+ add {escaped}/a.rs (production lines)")));
        assert!(summary.contains(&format!("+ add {escaped} (production direct files")));
    }
}

mod when_approving_one_new_file {
    use super::*;

    #[test]
    fn it_shows_only_the_scoped_addition_in_preview_and_write() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "legacy/lib.rs", 9);
        write_rust_source_with_function_count(&root, "src/new.rs", 2);
        let before = read_ratchet(&root);

        let preview = sprawl_guard(&root)
            .args(["update", "src"])
            .output()
            .unwrap();
        let after_preview = read_ratchet(&root);
        let write = sprawl_guard(&root)
            .args(["update", "src", "--write"])
            .output()
            .unwrap();

        assert_exit_code(&preview, exit_codes::SUCCESS);
        assert_exit_code(&write, exit_codes::SUCCESS);
        assert_eq!(after_preview, before);
        for output in [&preview, &write] {
            let summary = stdout(output);
            assert!(
                summary.contains("+ add src/new.rs (production lines): none -> 2"),
                "{summary}"
            );
            assert!(!summary.contains("legacy/lib.rs"));
        }
        assert_eq!(
            production_file_ratchet_entry(&read_ratchet_file(&root), "legacy/lib.rs")
                .unwrap()
                .max_lines
                .get(),
            9
        );
    }
}

mod when_an_existing_violation_grows {
    use super::*;

    #[test]
    fn it_reports_an_increase_only_with_explicit_permission() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "src/lib.rs", 2);
        write_rust_source_with_function_count(&root, "src/lib.rs", 3);

        let preserved = sprawl_guard(&root).arg("update").output().unwrap();
        let allowed = sprawl_guard(&root)
            .args(["update", "src/lib.rs", "--allow-increases"])
            .output()
            .unwrap();

        assert_exit_code(&preserved, exit_codes::SUCCESS);
        assert_eq!(stdout(&preserved), "No ratchet changes.\n");
        assert_exit_code(&allowed, exit_codes::SUCCESS);
        assert!(stdout(&allowed).contains("! raise src/lib.rs (production lines): 2 -> 3"));
    }
}

mod when_tightening_mixed_entries {
    use super::*;

    #[test]
    fn it_distinguishes_roles_groups_lowering_and_deletion() {
        let root = TempDir::new().unwrap();
        write_mixed_group_directory_direct_files_fixture(&root);
        write_ratchet(
            &root,
            indoc! {r#"
            schema_version = 1
            [[files]]
            path = "deleted.rs"
            role = "test"
            max_lines = 3
            [[directories]]
            path = "src"
            role = "production"
            group = "source"
            max_direct_files = 4
            [[directories]]
            path = "src"
            role = "production"
            group = "documentation"
            max_direct_files = 5
            [[directories]]
            path = "src"
            role = "test"
            group = "source"
            max_direct_files = 2
        "#},
        );

        let output = sprawl_guard(&root).arg("tighten").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        let summary = stdout(&output);
        for expected in [
            "- remove deleted.rs (test lines): 3 -> none",
            "- lower src (production direct files, group: source): 4 -> 2",
            "- lower src (production direct files, group: documentation): 5 -> 2",
            "- remove src (test direct files, group: source): 2 -> none",
        ] {
            assert!(summary.contains(expected), "{summary}");
        }
    }
}

mod when_no_ceilings_change {
    use super::*;

    #[test]
    fn it_preserves_the_existing_file_bytes_on_write() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "src/lib.rs", 2);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);
        let before = format!("# Keep my comment\n{}", read_ratchet(&root));
        write_ratchet(&root, &before);

        for command in ["update", "tighten"] {
            let output = sprawl_guard(&root)
                .args([command, "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(stdout(&output), "No ratchet changes.\n");
            assert_eq!(read_ratchet(&root), before);
        }
    }

    #[test]
    fn it_previews_then_creates_a_missing_empty_ratchet() {
        let root = TempDir::new().unwrap();
        write_config(&root, "");

        let preview = sprawl_guard(&root).args(["update"]).output().unwrap();

        assert_exit_code(&preview, exit_codes::SUCCESS);
        assert_eq!(
            stdout(&preview),
            "No ratchet changes; --write will create an empty ratchet file sprawl-guard.ratchet.toml\n"
        );
        assert!(!root.path().join("sprawl-guard.ratchet.toml").exists());

        let output = sprawl_guard(&root)
            .args(["update", "--write"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(read_ratchet_file(&root).is_empty());
        assert_eq!(
            stdout(&output),
            "Created empty ratchet file sprawl-guard.ratchet.toml\n"
        );
    }
}

mod when_full_toml_is_requested {
    use super::*;

    #[test]
    fn it_outputs_a_parseable_complete_document_even_when_writing() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "old.rs", 9);
        write_rust_source_with_function_count(&root, "new.rs", 2);

        let output = sprawl_guard(&root)
            .args(["update", "new.rs", "--format", "toml", "--write"])
            .output()
            .unwrap();
        let document = parse_toml(&stdout(&output));

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            document,
            toml_value(toml::toml! {
                schema_version = 1
                [[files]]
                path = "new.rs"
                role = "production"
                max_lines = 2
                [[files]]
                path = "old.rs"
                role = "production"
                max_lines = 9
            })
        );
    }

    mod and_quiet_is_requested {
        use super::*;

        #[test]
        fn it_suppresses_output_without_suppressing_the_write() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_rust_source_with_function_count(&root, "src/lib.rs", 2);

            let output = sprawl_guard(&root)
                .args(["update", "--format", "toml", "--write", "--quiet"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(stdout(&output), "");
            assert_eq!(read_ratchet_file(&root).files[0].max_lines.get(), 2);
        }
    }
}
