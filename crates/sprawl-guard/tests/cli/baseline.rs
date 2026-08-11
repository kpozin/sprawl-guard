use assert_fs::TempDir;
use indoc::indoc;
use sprawl_guard_lib::classification::SourceKind;

use super::common::*;

mod when_writing_the_initial_baseline {
    use super::*;

    #[test]
    fn it_creates_ratchet_entries_from_current_violations() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 1);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);

        let output = sprawl_guard(&root)
            .args(["baseline", "--write"])
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

        let (output, ratchet) = run_ratchet_file_write(&root, "baseline");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.directories.len(), 1);
        assert_eq!(ratchet.directories[0].group.as_str(), "source");
    }

    #[test]
    fn it_records_each_group_when_directory_entries_share_a_path() {
        let root = TempDir::new().unwrap();
        write_mixed_group_directory_fanout_fixture(&root);

        let (output, ratchet) = run_ratchet_file_write(&root, "baseline");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            production_directory_ratchet_entries(&ratchet),
            vec![("source", "src", 2), ("documentation", "src", 2),]
        );
    }

    #[test]
    fn it_excludes_a_custom_ratchet_from_create_and_replace_measurements() {
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
            .args(["baseline", "--write"])
            .output()
            .unwrap();
        let created = std::fs::read_to_string(root.path().join("custom-ratchet.toml")).unwrap();
        let replace = sprawl_guard(&root)
            .args(["baseline", "--all", "--write"])
            .output()
            .unwrap();
        let replaced = std::fs::read_to_string(root.path().join("custom-ratchet.toml")).unwrap();
        let check = sprawl_guard(&root).arg("check").output().unwrap();

        assert_exit_code(&create, exit_codes::SUCCESS);
        assert_exit_code(&replace, exit_codes::SUCCESS);
        assert_eq!(replaced, created);
        assert_exit_code(&check, exit_codes::SUCCESS);
    }
}

mod when_comparing_baseline_and_ratchet_help {
    use super::*;

    #[test]
    fn it_explains_which_command_can_create_raise_or_only_lower_ceilings() {
        let root = TempDir::new().unwrap();

        let baseline = sprawl_guard(&root)
            .args(["baseline", "--help"])
            .output()
            .unwrap();
        let ratchet = sprawl_guard(&root)
            .args(["ratchet", "--help"])
            .output()
            .unwrap();

        assert_exit_code(&baseline, exit_codes::SUCCESS);
        assert!(stdout(&baseline).contains("Create or replace ratchet ceilings"));
        assert_exit_code(&ratchet, exit_codes::SUCCESS);
        assert!(stdout(&ratchet).contains("never creates or raises ceilings"));
    }
}

mod when_the_ratchet_file_already_exists {
    use super::*;

    #[test]
    fn it_refuses_ambiguous_whole_file_replacement() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [languages.rust]
                enabled = true
            "#},
        );
        write_ratchet(&root, "schema_version = 1\n");

        let output = sprawl_guard(&root).arg("baseline").output().unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(stderr(&output).contains("ratchet file already exists"));
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
            .args(["baseline", "new", "--write"])
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

    #[test]
    fn it_refuses_to_raise_existing_ceilings_without_allow_raises() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "src/lib.rs", 2);
        write_file(
            &root,
            "src/lib.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
                fn c() {}
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["baseline", "src", "--write"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(stderr(&output).contains("refusing to raise 1 ratchet ceiling"));
    }

    #[test]
    fn it_removes_path_classified_test_entries_when_test_checks_are_disabled() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [tests]
                check_path_tests = false
            "#},
        );
        write_ratchet(
            &root,
            indoc! {r#"
                schema_version = 1

                [[files]]
                path = "tests/integration.rs"
                role = "test"
                max_loc = 2

                [[directories]]
                path = "tests"
                role = "test"
                group = "source"
                leaf_files = 1
            "#},
        );
        write_file(
            &root,
            "tests/integration.rs",
            "fn first() {}\nfn second() {}\n",
        );

        let output = sprawl_guard(&root)
            .args(["baseline", "--all", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(
            ratchet
                .files
                .iter()
                .all(|entry| entry.role != SourceKind::Test)
        );
        assert!(
            ratchet
                .directories
                .iter()
                .all(|entry| entry.role != SourceKind::Test)
        );
    }
}
