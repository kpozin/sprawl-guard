use assert_fs::TempDir;
use indoc::indoc;

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
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.version.get(), 1);
        assert_eq!(ratchet.files.len(), 1);
        let file = ratchet_file_entry(&ratchet, "src/lib.rs").unwrap();
        assert_eq!(file.non_test_code_lines.get(), 2);
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_records_the_automatic_source_group_for_directory_entries() {
        let root = TempDir::new().unwrap();
        write_rust_directory_fanout_fixture(&root);

        let output = sprawl_guard(&root)
            .args(["baseline", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.directories.len(), 1);
        assert_eq!(ratchet.directories[0].group.as_str(), "source");
    }

    #[test]
    fn it_records_each_group_when_directory_entries_share_a_path() {
        let root = TempDir::new().unwrap();
        write_mixed_group_directory_fanout_fixture(&root);

        let output = sprawl_guard(&root)
            .args(["baseline", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            directory_ratchet_entries(&ratchet),
            vec![("source", "src", 2), ("documentation", "src", 2),]
        );
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
        write_ratchet(&root, "version = 1\n");

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
                code_non_test = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_ratchet(
            &root,
            indoc! {r#"
                version = 1

                [[files]]
                path = "old/lib.rs"
                non_test_code_lines = 9
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
        let old_file = ratchet_file_entry(&ratchet, "old/lib.rs").unwrap();
        assert_eq!(old_file.non_test_code_lines.get(), 9);
        let new_file = ratchet_file_entry(&ratchet, "new/lib.rs").unwrap();
        assert_eq!(new_file.non_test_code_lines.get(), 2);
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
}
