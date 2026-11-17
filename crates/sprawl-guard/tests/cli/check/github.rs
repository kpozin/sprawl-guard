use assert_fs::TempDir;
use indoc::indoc;

use super::super::common::*;

/// Annotation message for the two-line `src/lib.rs` that these tests check against a one-line
/// limit.
const ANNOTATION_MESSAGE: &str =
    "title=sprawl-guard/file-lines::src/lib.rs has 2 lines (counting code), over the limit of 1.";

/// Asserts that stdout holds the human report, with workflow commands stopped around it, followed
/// by one annotation for `file`.
fn assert_human_report_then_annotation(stdout: &str, file: &str) {
    let (fenced_report, annotation) = stdout
        .trim_end()
        .rsplit_once('\n')
        .expect("output has a report and an annotation");
    let (stop, report) = fenced_report
        .split_once('\n')
        .expect("output stops workflow commands first");
    let token = stop
        .strip_prefix("::stop-commands::")
        .expect("output stops workflow commands first");
    let report = report
        .strip_suffix(&format!("\n::{token}::"))
        .expect("output resumes workflow commands before the annotation");
    assert!(report.contains("Files over the line limit:\n\n  src/lib.rs: 2 lines (limit 1)"));
    assert_eq!(
        annotation,
        format!("::error file={file},line=1,{ANNOTATION_MESSAGE}")
    );
}

fn write_over_limit_source(root: &TempDir) {
    write_rust_source_with_function_count(root, "src/lib.rs", 2);
}

mod when_github_output_is_requested {
    use super::*;

    mod and_the_check_runs_outside_github_actions {
        use super::*;

        #[test]
        fn it_prints_the_human_report_followed_by_root_relative_annotations() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_over_limit_source(&root);

            let output = sprawl_guard(&root)
                .args(["check", "--format", "github"])
                .env_remove("GITHUB_WORKSPACE")
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_human_report_then_annotation(&stdout(&output), "src/lib.rs");
            assert_eq!(stderr(&output), "");
        }
    }

    mod and_a_path_cannot_be_read {
        use super::*;

        #[test]
        fn it_stops_workflow_commands_around_the_report_on_stderr() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_file(&root, "##[error]forged/.ignore", "!{lib.rs\n");

            let output = sprawl_guard(&root)
                .args(["check", "--format", "github"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
            assert_eq!(stdout(&output), "");
            let stderr = stderr(&output);
            let (stop, rest) = stderr.split_once('\n').unwrap();
            let token = stop.strip_prefix("::stop-commands::").unwrap();
            assert!(rest.contains("##[error]forged/.ignore"), "{stderr}");
            assert!(rest.ends_with(&format!("\n::{token}::\n")), "{stderr}");
        }
    }

    mod and_the_root_is_a_subdirectory_of_the_workspace {
        use super::*;

        #[test]
        fn it_names_files_relative_to_the_workspace() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_over_limit_source(&root);
            let workspace = root.path().parent().unwrap();
            let root_name = root.path().file_name().unwrap().to_str().unwrap();

            let output = sprawl_guard(&root)
                .args(["check", "--format", "github"])
                .env("GITHUB_WORKSPACE", workspace)
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_human_report_then_annotation(
                &stdout(&output),
                &format!("{root_name}/src/lib.rs"),
            );
        }
    }
}

mod when_the_config_selects_github_output {
    use super::*;

    #[test]
    fn it_prints_annotations_without_a_format_flag() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [output]
                format = "github"

                [limits.max_lines_per_file]
                production = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_over_limit_source(&root);

        let output = sprawl_guard(&root)
            .arg("check")
            .env_remove("GITHUB_WORKSPACE")
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_human_report_then_annotation(&stdout(&output), "src/lib.rs");
    }
}
