use assert_fs::TempDir;
use indoc::indoc;

use super::common::*;

mod preview;

mod when_writing_the_initial_update {
    use super::*;

    #[test]
    fn it_creates_ratchet_entries_from_current_violations() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 1);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);

        let output = sprawl_guard(&root)
            .args(["update", "--write"])
            .output()
            .unwrap();
        let document = parse_toml(&read_ratchet(&root));

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            document,
            parse_toml(indoc! {r#"
                schema_version = 1

                [[files]]
                path = "src/lib.rs"
                role = "production"
                max_loc = 2
            "#})
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_records_the_automatic_source_group_for_directory_entries() {
        let root = TempDir::new().unwrap();
        write_rust_directory_fanout_fixture(&root);

        let (output, ratchet) = run_ratchet_file_write(&root, "update");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.directories.len(), 1);
        assert_eq!(ratchet.directories[0].group.as_str(), "source");
    }

    #[test]
    fn it_records_each_group_when_directory_entries_share_a_path() {
        let root = TempDir::new().unwrap();
        write_mixed_group_directory_fanout_fixture(&root);

        let (output, ratchet) = run_ratchet_file_write(&root, "update");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            production_directory_ratchet_entries(&ratchet),
            vec![("source", "src", 2), ("documentation", "src", 2),]
        );
    }

    #[test]
    fn it_excludes_a_custom_ratchet_from_repeated_measurements() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [ratchet]
                file = "custom-ratchet.toml"
            "#},
        );
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);

        let create = sprawl_guard(&root)
            .args(["update", "--write"])
            .output()
            .unwrap();
        let created = std::fs::read_to_string(root.path().join("custom-ratchet.toml")).unwrap();
        let repeat = sprawl_guard(&root)
            .args(["update", "--write"])
            .output()
            .unwrap();
        let repeated = std::fs::read_to_string(root.path().join("custom-ratchet.toml")).unwrap();
        let check = sprawl_guard(&root).arg("check").output().unwrap();

        assert_exit_code(&create, exit_codes::SUCCESS);
        assert_exit_code(&repeat, exit_codes::SUCCESS);
        assert_eq!(repeated, created);
        assert_exit_code(&check, exit_codes::SUCCESS);
    }

    #[test]
    fn it_does_not_create_entries_for_dependency_lockfiles() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [languages.yaml]
                enabled = true
            "#},
        );
        write_file(&root, "config.yaml", "first: value\nsecond: value\n");
        write_file(
            &root,
            "pnpm-lock.yaml",
            "lockfileVersion: '9.0'\npackages:\n  dependency: {}\n",
        );

        let (output, ratchet) = run_ratchet_file_write(&root, "update");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 1);
        assert_eq!(ratchet.files[0].path.as_str(), "config.yaml");
        assert_eq!(stderr(&output), "");
    }
}

mod when_previewing_the_initial_update {
    use super::*;

    #[test]
    fn it_prints_a_ratchet_document_with_its_editor_schema() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 1);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);

        let output = sprawl_guard(&root)
            .args(["update", "--format", "toml"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout(&output).contains(concat!(
            "#:schema https://kpozin.github.io/sprawl-guard/schemas/v1/",
            "sprawl-guard.ratchet.schema.json\n",
        )));
        assert_eq!(stderr(&output), "");
    }
}

mod when_comparing_update_and_tighten_help {
    use super::*;

    #[test]
    fn it_explains_which_command_can_create_raise_or_only_lower_ceilings() {
        let root = TempDir::new().unwrap();

        let update = sprawl_guard(&root)
            .args(["update", "--help"])
            .output()
            .unwrap();
        let tighten = sprawl_guard(&root)
            .args(["tighten", "--help"])
            .output()
            .unwrap();

        assert_exit_code(&update, exit_codes::SUCCESS);
        assert!(stdout(&update).contains("Add ratchet ceilings for newly discovered violations"));
        assert!(stdout(&update).contains("--allow-increases"));
        assert_exit_code(&tighten, exit_codes::SUCCESS);
        assert!(stdout(&tighten).contains("Lower or remove existing ratchet ceilings"));
    }
}

mod when_the_ratchet_file_already_exists {
    use super::*;

    #[test]
    fn it_accepts_an_unscoped_update_and_preserves_existing_entries() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 10

                [languages.rust]
                enabled = true
            "#},
        );
        write_ratchet(
            &root,
            indoc! {r#"
                schema_version = 1

                [[files]]
                path = "src/now-small.rs"
                role = "production"
                max_loc = 20
            "#},
        );
        write_file(&root, "src/now-small.rs", "fn small() {}\n");

        let (output, ratchet) = run_ratchet_file_write(&root, "update");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 1);
        assert_eq!(ratchet.files[0].path.as_str(), "src/now-small.rs");
        assert_eq!(ratchet.files[0].max_loc.get(), 20);
        assert!(ratchet.directories.is_empty());
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_preserves_out_of_scope_entries_for_scoped_writes() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_ratchet(
            &root,
            indoc! {r#"
                schema_version = 1

                [[files]]
                path = "old/lib.rs"
                role = "production"
                max_loc = 9
            "#},
        );
        write_file(
            &root,
            "new/lib.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["update", "new", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 2);
        let old_file = production_file_ratchet_entry(&ratchet, "old/lib.rs").unwrap();
        assert_eq!(old_file.max_loc.get(), 9);
        let new_file = production_file_ratchet_entry(&ratchet, "new/lib.rs").unwrap();
        assert_eq!(new_file.max_loc.get(), 2);
    }

    fn assert_scoped_update_ceiling(flags: &[&str], expected_ceiling: u32) {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "src/lib.rs", 2);
        write_rust_source_with_function_count(&root, "src/lib.rs", 3);

        let output = sprawl_guard(&root)
            .args(["update", "src"])
            .args(flags)
            .arg("--write")
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files[0].max_loc.get(), expected_ceiling);
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_preserves_existing_ceilings_without_allow_increases() {
        assert_scoped_update_ceiling(&[], 2);
    }

    #[test]
    fn it_raises_existing_ceilings_when_explicitly_allowed() {
        assert_scoped_update_ceiling(&["--allow-increases"], 3);
    }
}

#[cfg(unix)]
mod when_traversal_is_incomplete {
    use super::*;

    fn configure_warning_only_traversal(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [traversal]
                follow_symlinks = "directories"
                error_policy = "warn"

                [languages.rust]
                enabled = true
            "#},
        );
    }

    #[test]
    fn it_rejects_preview_and_write_without_creating_a_ratchet() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);
        configure_warning_only_traversal(&root);

        let preview = sprawl_guard(&root).arg("update").output().unwrap();
        let write = sprawl_guard(&root)
            .args(["update", "--write"])
            .output()
            .unwrap();

        for output in [&preview, &write] {
            assert_exit_code(output, exit_codes::OPERATIONAL_ERROR);
            assert_eq!(stdout(output), "");
            let error = stderr(output);
            assert!(
                error.contains("symlink loop found at real/cycle"),
                "{error}"
            );
        }
        assert!(!root.path().join("sprawl-guard.ratchet.toml").exists());
    }

    #[test]
    fn it_ignores_traversal_failures_outside_a_scoped_update() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);
        configure_warning_only_traversal(&root);
        write_rust_source_with_function_count(&root, "safe/lib.rs", 2);

        let output = sprawl_guard(&root)
            .args(["update", "safe", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 1);
        assert_eq!(ratchet.files[0].path.as_str(), "safe/lib.rs");
    }
}
