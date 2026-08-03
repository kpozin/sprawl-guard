use assert_fs::TempDir;
use indoc::indoc;

use super::common::*;

mod when_writing_updates {
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
                loc = 4

                [[files]]
                path = "src/obsolete.rs"
                role = "production"
                loc = 3
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

        let (output, ratchet) = run_ratchet_file_write(&root, "ratchet");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 1);
        let improved_file = production_file_ratchet_entry(&ratchet, "src/improved.rs").unwrap();
        assert_eq!(improved_file.loc.get(), 2);
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
                loc = 4

                [[files]]
                path = "skipped/lib.rs"
                role = "production"
                loc = 4
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
            .args(["ratchet", "checked", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 2);
        let checked_file = production_file_ratchet_entry(&ratchet, "checked/lib.rs").unwrap();
        assert_eq!(checked_file.loc.get(), 2);
        let skipped_file = production_file_ratchet_entry(&ratchet, "skipped/lib.rs").unwrap();
        assert_eq!(skipped_file.loc.get(), 4);
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
                loc = 4
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["ratchet", "deleted", "--write"])
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

        let (output, ratchet) = run_ratchet_file_write(&root, "ratchet");

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
                loc = 4

                [[files]]
                path = "src/lib.rs"
                role = "test"
                loc = 4
            "#},
        );
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);

        let output = sprawl_guard(&root)
            .args(["ratchet", "--write"])
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
        assert_eq!(ratchet.files[0].loc.get(), 2);
    }
}
