use assert_fs::TempDir;
use indoc::formatdoc;

use super::super::common::*;

/// Most files these fixtures put in one directory, below the default direct-file limit, so the
/// only findings are files over the line limit.
const FILES_PER_DIRECTORY: usize = 9;

/// Built-in finding cap, which `check` uses when neither the flag nor the config sets one.
const DEFAULT_MAX_FINDINGS: usize = 50;

/// Writes a config with a one-line limit for production and test Rust files, plus any extra
/// `[output]` lines.
fn write_config_with_output(root: &TempDir, output: &str) {
    write_config(
        root,
        &formatdoc! {r#"
            [output]
            {output}

            [limits.max_lines_per_file]
            production = 1
            test = 1

            [languages.rust]
            enabled = true
        "#},
    );
}

/// Writes `count` two-line Rust files under `prefix`, each over the one-line limit.
fn write_files_over_limit(root: &TempDir, prefix: &str, count: usize) {
    for index in 0..count {
        let directory = index / FILES_PER_DIRECTORY;
        write_rust_source_with_function_count(
            root,
            &format!("{prefix}{directory}/file{index:03}.rs"),
            2,
        );
    }
}

fn check(root: &TempDir, args: &[&str]) -> std::process::Output {
    sprawl_guard(root)
        .arg("check")
        .args(args)
        .env_remove("GITHUB_WORKSPACE")
        .output()
        .unwrap()
}

/// Counts the violations that the human report lists.
fn listed_violations(stdout: &str) -> usize {
    stdout.matches(": 2 lines (limit 1)").count()
}

mod when_findings_exceed_the_default_cap {
    use super::*;

    #[test]
    fn it_lists_the_default_number_and_shows_the_total() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "");
        write_files_over_limit(&root, "src", DEFAULT_MAX_FINDINGS + 1);

        let output = check(&root, &[]);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        let stdout = stdout(&output);
        assert_eq!(listed_violations(&stdout), DEFAULT_MAX_FINDINGS);
        assert!(stdout.contains("Files over the line limit (51):\n"));
        assert!(stdout.contains("  … and 1 more\n"));
        assert!(
            stdout.ends_with(
                "\nShowing 50 of 51 findings. Use --max-findings unlimited to see all.\n"
            )
        );
    }
}

mod when_the_flag_sets_a_number {
    use super::*;

    #[test]
    fn it_lists_that_many_findings_and_still_fails() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "");
        write_files_over_limit(&root, "src", 3);

        let output = check(&root, &["--max-findings", "2"]);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        let stdout = stdout(&output);
        assert_eq!(listed_violations(&stdout), 2);
        assert!(stdout.ends_with(
            "  … and 1 more\n\nShowing 2 of 3 findings. Use --max-findings unlimited to see all.\n"
        ));
    }

    mod and_the_config_sets_another {
        use super::*;

        #[test]
        fn it_uses_the_flag() {
            let root = TempDir::new().unwrap();
            write_config_with_output(&root, "max_findings = 1");
            write_files_over_limit(&root, "src", 3);

            let output = check(&root, &["--max-findings", "2"]);

            assert_eq!(listed_violations(&stdout(&output)), 2);
        }
    }
}

mod when_the_config_sets_a_number {
    use super::*;

    #[test]
    fn it_lists_that_many_findings() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "max_findings = 1");
        write_files_over_limit(&root, "src", 3);

        let output = check(&root, &[]);

        assert_eq!(listed_violations(&stdout(&output)), 1);
    }
}

mod when_the_cap_is_unlimited {
    use super::*;

    #[test]
    fn it_lists_every_finding_without_a_footer() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "max_findings = 1");
        write_files_over_limit(&root, "src", DEFAULT_MAX_FINDINGS + 1);

        let output = check(&root, &["--max-findings", "unlimited"]);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        let stdout = stdout(&output);
        assert_eq!(listed_violations(&stdout), DEFAULT_MAX_FINDINGS + 1);
        assert!(!stdout.contains("more\n"));
        assert!(!stdout.contains("Showing"));
    }
}

mod when_the_config_sets_unlimited {
    use super::*;

    #[test]
    fn it_lists_every_finding() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, r#"max_findings = "unlimited""#);
        write_files_over_limit(&root, "src", DEFAULT_MAX_FINDINGS + 1);

        let output = check(&root, &[]);

        assert_eq!(
            listed_violations(&stdout(&output)),
            DEFAULT_MAX_FINDINGS + 1
        );
    }
}

mod when_the_cap_is_zero {
    use super::*;

    #[test]
    fn it_rejects_the_flag() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "");

        let output = check(&root, &["--max-findings", "0"]);

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(stderr(&output).contains("finding cap must be greater than zero"));
    }
}

mod when_the_cap_runs_out_before_a_group {
    use super::*;

    #[test]
    fn it_shows_the_group_heading_and_total_without_entries() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "");
        write_files_over_limit(&root, "src", 1);
        write_files_over_limit(&root, "tests/", 2);

        let output = check(&root, &["--max-findings", "1"]);

        assert!(stdout(&output).contains(
            "Test files over the line limit (2):\n\n  … and 2 more\n\nShowing 1 of 3 findings."
        ));
    }
}

mod when_github_output_is_capped {
    use super::*;

    #[test]
    fn it_annotates_only_the_listed_findings() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "");
        write_files_over_limit(&root, "src", 3);

        let output = check(&root, &["--format", "github", "--max-findings", "1"]);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        let stdout = stdout(&output);
        assert_eq!(stdout.matches("\n::error ").count(), 1);
        assert!(stdout.contains("::error file=src0/file000.rs,"));
    }
}

mod when_machine_output_is_requested_with_a_cap {
    use super::*;

    #[test]
    fn it_reports_every_violation_as_json() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "");
        write_files_over_limit(&root, "src", 3);

        let output = check(&root, &["--format", "json", "--max-findings", "1"]);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            json_stdout(&output)["violations"].as_array().unwrap().len(),
            3
        );
    }

    #[test]
    fn it_reports_every_violation_as_sarif() {
        let root = TempDir::new().unwrap();
        write_config_with_output(&root, "");
        write_files_over_limit(&root, "src", 3);

        let output = check(&root, &["--format", "sarif", "--max-findings", "1"]);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            json_stdout(&output)["runs"][0]["results"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }
}
