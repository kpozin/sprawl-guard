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
                path = "src/improved.rs"
                non_test_code_lines = 4

                [[files]]
                path = "src/obsolete.rs"
                non_test_code_lines = 3
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

        let output = sprawl_guard(&root)
            .args(["ratchet", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(ratchet.files.len(), 1);
        let improved_file = ratchet_file_entry(&ratchet, "src/improved.rs").unwrap();
        assert_eq!(improved_file.non_test_code_lines.get(), 2);
        assert!(ratchet_file_entry(&ratchet, "src/obsolete.rs").is_none());
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_preserves_out_of_scope_entries() {
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
                path = "checked/lib.rs"
                non_test_code_lines = 4

                [[files]]
                path = "skipped/lib.rs"
                non_test_code_lines = 4
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
        let checked_file = ratchet_file_entry(&ratchet, "checked/lib.rs").unwrap();
        assert_eq!(checked_file.non_test_code_lines.get(), 2);
        let skipped_file = ratchet_file_entry(&ratchet, "skipped/lib.rs").unwrap();
        assert_eq!(skipped_file.non_test_code_lines.get(), 4);
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_removes_a_deleted_relative_scope() {
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
                path = "deleted/lib.rs"
                non_test_code_lines = 4
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["ratchet", "deleted", "--write"])
            .output()
            .unwrap();
        let ratchet = read_ratchet_file(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(ratchet_file_entry(&ratchet, "deleted/lib.rs").is_none());
        assert_eq!(stderr(&output), "");
    }
}
