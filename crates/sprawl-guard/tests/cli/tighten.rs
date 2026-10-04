use assert_fs::TempDir;
use indoc::indoc;

use super::common::*;

mod when_tightening_entries {
    use super::*;

    #[test]
    fn it_lowers_improved_entries_and_removes_obsolete_entries() {
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
                path = "src/improved.rs"
                role = "production"
                max_loc = 4

                [[files]]
                path = "src/obsolete.rs"
                role = "production"
                max_loc = 3
            "#},
        );
        write_file(
            &root,
            "src/improved.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
            "#},
        );
        write_file(&root, "src/obsolete.rs", "fn a() {}\n");

        let (output, ratchet) = run_ratchet_file_write(&root, "tighten");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 1);
        let improved_file = production_file_ratchet_entry(&ratchet, "src/improved.rs").unwrap();
        assert_eq!(improved_file.max_loc.get(), 2);
        assert!(production_file_ratchet_entry(&ratchet, "src/obsolete.rs").is_none());
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_preserves_out_of_scope_entries() {
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
                path = "checked/lib.rs"
                role = "production"
                max_loc = 4

                [[files]]
                path = "skipped/lib.rs"
                role = "production"
                max_loc = 4
            "#},
        );
        write_file(
            &root,
            "checked/lib.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
            "#},
        );
        write_file(
            &root,
            "skipped/lib.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
                fn c() {}
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["tighten", "checked", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 2);
        let checked_file = production_file_ratchet_entry(&ratchet, "checked/lib.rs").unwrap();
        assert_eq!(checked_file.max_loc.get(), 2);
        let skipped_file = production_file_ratchet_entry(&ratchet, "skipped/lib.rs").unwrap();
        assert_eq!(skipped_file.max_loc.get(), 4);
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_removes_a_deleted_relative_scope() {
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
                path = "deleted/lib.rs"
                role = "production"
                max_loc = 4
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["tighten", "deleted", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(production_file_ratchet_entry(&ratchet, "deleted/lib.rs").is_none());
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_lowers_each_group_when_directory_entries_share_a_path() {
        let root = TempDir::new().unwrap();
        write_mixed_group_directory_fanout_fixture(&root);
        write_ratchet(
            &root,
            indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                leaf_files = 3

                [[directories]]
                path = "src"
                role = "production"
                group = "documentation"
                leaf_files = 3
            "#},
        );

        let (output, ratchet) = run_ratchet_file_write(&root, "tighten");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            production_directory_ratchet_entries(&ratchet),
            vec![("source", "src", 2), ("documentation", "src", 2),]
        );
    }

    #[test]
    fn it_updates_only_the_role_currently_assigned_to_a_path() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [tests]
                globs = ["src/lib.rs"]

                [limits.max_lines_per_file]
                test = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_ratchet(
            &root,
            indoc! {r#"
                schema_version = 1

                [[files]]
                path = "src/lib.rs"
                role = "production"
                max_loc = 4

                [[files]]
                path = "src/lib.rs"
                role = "test"
                max_loc = 4
            "#},
        );
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);

        let output = sprawl_guard(&root)
            .args(["tighten", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 1);
        assert_eq!(ratchet.files[0].path.as_str(), "src/lib.rs");
        assert_eq!(
            ratchet.files[0].role,
            sprawl_guard_lib::classification::SourceKind::Test
        );
        assert_eq!(ratchet.files[0].max_loc.get(), 2);
    }

    #[test]
    fn it_neither_raises_existing_entries_nor_creates_new_entries() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "src/existing.rs", 2);
        write_rust_source_with_function_count(&root, "src/existing.rs", 3);
        write_rust_source_with_function_count(&root, "src/new.rs", 2);

        let (output, ratchet) = run_ratchet_file_write(&root, "tighten");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 1);
        assert_eq!(ratchet.files[0].path.as_str(), "src/existing.rs");
        assert_eq!(ratchet.files[0].max_loc.get(), 2);
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_removes_dependency_lockfile_entries_and_inflated_directory_counts() {
        let root = TempDir::new().unwrap();
        write_dependency_lockfile_ratchet_fixture(&root);

        let (output, ratchet) = run_ratchet_file_write(&root, "tighten");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(ratchet.files.is_empty());
        assert!(ratchet.directories.is_empty());
        assert_eq!(stderr(&output), "");
    }
}

#[cfg(unix)]
mod when_traversal_is_incomplete {
    use super::*;

    #[test]
    fn it_rejects_preview_and_write_without_changing_the_ratchet() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [limits.max_leaf_files_per_directory]
                production = 1

                [traversal]
                follow_symlinks = "directories"
                error_policy = "warn"

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(&root, "real/other.rs", "fn other() {}\n");
        let original = indoc! {r#"
            schema_version = 1

            [[files]]
            path = "real/lib.rs"
            role = "production"
            max_loc = 4

            [[directories]]
            path = "real"
            role = "production"
            group = "source"
            leaf_files = 4
        "#};
        write_ratchet(&root, original);

        let preview = sprawl_guard(&root).arg("tighten").output().unwrap();
        let write = sprawl_guard(&root)
            .args(["tighten", "--write"])
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
        assert_eq!(read_ratchet(&root), original);
    }

    #[test]
    fn it_ignores_traversal_failures_outside_a_scoped_tighten() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);
        write_config(
            &root,
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
        write_rust_source_with_function_count(&root, "safe/lib.rs", 2);
        write_ratchet(
            &root,
            indoc! {r#"
                schema_version = 1

                [[files]]
                path = "safe/lib.rs"
                role = "production"
                max_loc = 4

                [[files]]
                path = "real/lib.rs"
                role = "production"
                max_loc = 4
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["tighten", "safe", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            production_file_ratchet_entry(&ratchet, "safe/lib.rs")
                .unwrap()
                .max_loc
                .get(),
            2
        );
        assert_eq!(
            production_file_ratchet_entry(&ratchet, "real/lib.rs")
                .unwrap()
                .max_loc
                .get(),
            4
        );
        assert_eq!(stderr(&output), "");
    }
}
