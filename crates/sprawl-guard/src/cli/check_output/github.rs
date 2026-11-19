//! GitHub Actions annotations for check violations.
//!
//! GitHub Actions reads [workflow commands] from a step's output. Each `::error` line becomes an
//! annotation on the workflow run, and one that names a file also appears beside that file in a
//! pull request's diff. Reports that name repository files are printed as plain log text.
//!
//! [workflow commands]: https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions

use std::collections::hash_map::RandomState;
use std::env;
use std::hash::{BuildHasher, Hasher};
use std::path::{Component, Path};

use sprawl_guard_lib::CheckReport;
use sprawl_guard_lib::check::CheckViolation;

use super::sarif::{Finding, finding};
use super::violations_in_display_order;

/// The line that file annotations point at. A size limit applies to the whole file, and GitHub
/// shows an annotation in a diff only when it names a line.
const FILE_ANNOTATION_LINE: u32 = 1;

/// The environment variable that holds the checkout directory in a GitHub Actions job.
const GITHUB_WORKSPACE: &str = "GITHUB_WORKSPACE";

/// How file annotations name their files.
#[derive(Debug, Eq, PartialEq)]
enum FilePaths {
    /// Each file's path is this prefix followed by its root-relative path. The prefix is the
    /// root's path inside the workspace, ending in `/`, or empty.
    Prefixed(String),
    /// The root is outside the workspace, so no path would match a file in the repository. File
    /// violations become annotations without a file, and their messages still name the file.
    OutsideWorkspace,
}

/// Renders the human report as plain log text, followed by one `::error` workflow command for
/// each of the first `listed` violations in display order, which are the ones the report lists.
pub(super) fn github_check_report(
    human_report: &str,
    report: &CheckReport,
    listed: usize,
) -> String {
    let root = Path::new(report.root());
    let workspace =
        env::var_os(GITHUB_WORKSPACE).and_then(|path| Path::new(&path).canonicalize().ok());
    plain_log_text(human_report)
        + &annotations(report, &file_paths(root, workspace.as_deref()), listed)
}

/// Prints text, such as a report that names repository files, so that the runner shows it without
/// running workflow commands in it.
pub(super) fn plain_log_text(text: &str) -> String {
    fenced(text, &stop_token())
}

/// Returns a token that nobody writing the repository's files can predict.
///
/// The standard library seeds each `RandomState` from the operating system's random source.
fn stop_token() -> String {
    format!(
        "sprawl-guard-{:016x}",
        RandomState::new().build_hasher().finish()
    )
}

/// Wraps text between `::stop-commands::` and its resume token, so that the runner prints it
/// without running workflow commands in it.
///
/// The human report names repository files, and the runner finds commands after leading spaces
/// and, in the legacy `##[` form, anywhere in a line. Escaping can't rule out both.
fn fenced(text: &str, token: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let newline = if text.ends_with('\n') { "" } else { "\n" };
    format!("::stop-commands::{token}\n{text}{newline}::{token}::\n")
}

fn annotations(report: &CheckReport, file_paths: &FilePaths, listed: usize) -> String {
    violations_in_display_order(report)
        .into_iter()
        .take(listed)
        .map(|violation| annotation(file_paths, violation))
        .collect()
}

/// Decides how annotations name files under `root`.
///
/// GitHub matches an annotation's file against the repository's own paths, which are relative to
/// the workspace. Both paths are canonical, so the comparison holds through symlinked workspaces
/// and Windows path prefixes. Without a workspace, file paths stay relative to the root, which is
/// right when the root is the checkout.
fn file_paths(root: &Path, workspace: Option<&Path>) -> FilePaths {
    let Some(workspace) = workspace else {
        return FilePaths::Prefixed(String::new());
    };
    let Ok(relative) = root.strip_prefix(workspace) else {
        return FilePaths::OutsideWorkspace;
    };
    FilePaths::Prefixed(
        relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(format!("{}/", name.to_string_lossy())),
                _ => None,
            })
            .collect(),
    )
}

/// Renders one violation's workflow command.
///
/// Only file violations name a file, and only when GitHub can place it. Other annotations appear
/// on the workflow run alone.
fn annotation(file_paths: &FilePaths, violation: &CheckViolation) -> String {
    let Finding { rule_id, message } = finding(violation);
    let file = match (violation, file_paths) {
        (CheckViolation::FileLines(violation), FilePaths::Prefixed(prefix)) => format!(
            "file={},line={FILE_ANNOTATION_LINE},",
            escape_property(&format!("{prefix}{}", violation.path))
        ),
        (CheckViolation::FileLines(_), FilePaths::OutsideWorkspace)
        | (CheckViolation::DirectoryDirectFiles(_) | CheckViolation::Ratchet(_), _) => {
            String::new()
        }
    };
    format!(
        "::error {file}title={}::{}\n",
        escape_property(rule_id),
        escape_data(&message)
    )
}

/// Escapes a workflow command's message, as the GitHub Actions toolkit does.
fn escape_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a workflow command property value, which also can't contain `:` or `,`.
fn escape_property(value: &str) -> String {
    escape_data(value).replace(':', "%3A").replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use sprawl_guard_lib::check::{DirectoryViolationKind, FileViolationKind, RatchetViolation};
    use sprawl_guard_lib::{
        DirectoryRatchetStaleMissingViolation, LanguageGroupId, RatchetCount,
        RatchetStaleMissingViolation, RelativePath,
    };

    use super::super::sarif::tests::{
        directory_direct_files_violation, file_lines_violation, report_with,
    };
    use super::*;

    /// Renders annotations for a check whose root is the workspace.
    fn root_annotations(report: &CheckReport) -> String {
        annotations(
            report,
            &FilePaths::Prefixed(String::new()),
            report.violations().len(),
        )
    }

    fn file_report(kind: FileViolationKind, path: &str) -> CheckReport {
        report_with(vec![CheckViolation::FileLines(file_lines_violation(
            kind, path,
        ))])
    }

    fn directory_report(path: &str) -> CheckReport {
        report_with(vec![CheckViolation::DirectoryDirectFiles(
            directory_direct_files_violation(
                DirectoryViolationKind::DirectoryDirectFiles,
                path,
                vec![],
            ),
        )])
    }

    mod when_a_file_is_over_its_limit {
        use super::*;

        #[test]
        fn it_annotates_the_first_line_of_the_file() {
            assert_eq!(
                root_annotations(&file_report(FileViolationKind::FileLines, "src/lib.rs")),
                "::error file=src/lib.rs,line=1,title=sprawl-guard/file-lines::\
                 src/lib.rs has 3 lines (counting code), over the limit of 2.\n"
            );
        }

        #[test]
        fn it_uses_the_test_file_rule_for_a_test_file() {
            let report = file_report(FileViolationKind::TestFileLines, "tests/it.rs");

            assert!(root_annotations(&report).contains("title=sprawl-guard/test-file-lines::"));
        }

        #[test]
        fn it_escapes_characters_that_end_a_property_or_command() {
            assert_eq!(
                root_annotations(&file_report(
                    FileViolationKind::FileLines,
                    "src/a,b:c%d\ne.rs"
                )),
                "::error file=src/a%2Cb%3Ac%25d%0Ae.rs,line=1,\
                 title=sprawl-guard/file-lines::\
                 src/a,b:c%25d%0Ae.rs has 3 lines (counting code), over the limit of 2.\n"
            );
        }
    }

    mod when_a_directory_has_too_many_files {
        use super::*;

        #[test]
        fn it_annotates_the_run_without_naming_a_file() {
            assert_eq!(
                root_annotations(&directory_report("src")),
                "::error title=sprawl-guard/directory-direct-files::\
                 src in group source has 2 direct production files, over the limit of 1.\n"
            );
        }

        #[test]
        fn it_names_the_repository_root_as_a_dot() {
            assert!(root_annotations(&directory_report("")).contains("::. in group source has"));
        }
    }

    mod when_a_ratchet_entry_is_stale {
        use super::*;

        #[test]
        fn it_annotates_the_run_without_naming_a_file() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Missing(
                RatchetStaleMissingViolation::Directory(DirectoryRatchetStaleMissingViolation {
                    kind: DirectoryViolationKind::DirectoryDirectFiles,
                    path: RelativePath::new("src").unwrap(),
                    group: LanguageGroupId::Source,
                    ratchet: RatchetCount::new(4),
                }),
            ))]);

            assert_eq!(
                root_annotations(&report),
                "::error title=sprawl-guard/ratchet-stale::\
                 src in group source has a stale ratchet entry: ceiling 4, and it was not found.\n"
            );
        }
    }

    mod when_the_root_is_inside_the_workspace {
        use super::*;

        #[test]
        fn it_names_files_relative_to_the_workspace() {
            let report = file_report(FileViolationKind::FileLines, "src/lib.rs");
            let file_paths = file_paths(
                Path::new("/workspace/tools/app"),
                Some(Path::new("/workspace")),
            );

            assert!(
                annotations(&report, &file_paths, 1)
                    .starts_with("::error file=tools/app/src/lib.rs,")
            );
        }

        #[test]
        fn it_names_files_relative_to_the_root_when_the_root_is_the_workspace() {
            assert_eq!(
                file_paths(Path::new("/workspace"), Some(Path::new("/workspace"))),
                FilePaths::Prefixed(String::new())
            );
        }
    }

    mod when_the_root_is_outside_the_workspace {
        use super::*;

        #[test]
        fn it_annotates_file_violations_without_a_file() {
            let report = file_report(FileViolationKind::FileLines, "src/lib.rs");
            let file_paths = file_paths(Path::new("/elsewhere"), Some(Path::new("/workspace")));

            assert_eq!(
                annotations(&report, &file_paths, 1),
                "::error title=sprawl-guard/file-lines::\
                 src/lib.rs has 3 lines (counting code), over the limit of 2.\n"
            );
        }
    }

    mod when_there_is_no_workspace {
        use super::*;

        #[test]
        fn it_names_files_relative_to_the_root() {
            assert_eq!(
                file_paths(Path::new("/repo"), None),
                FilePaths::Prefixed(String::new())
            );
        }
    }

    mod when_fencing_the_human_report {
        use super::*;

        #[test]
        fn it_stops_workflow_commands_until_the_token() {
            assert_eq!(
                fenced("  ::error::forged\n", "token"),
                "::stop-commands::token\n  ::error::forged\n::token::\n"
            );
        }

        #[test]
        fn it_ends_the_text_with_a_newline_before_the_token() {
            assert_eq!(
                fenced("text", "token"),
                "::stop-commands::token\ntext\n::token::\n"
            );
        }

        #[test]
        fn it_prints_nothing_for_an_empty_report() {
            assert_eq!(fenced("", "token"), "");
        }

        #[test]
        fn it_uses_a_different_token_each_time() {
            assert_ne!(stop_token(), stop_token());
        }
    }

    mod when_there_are_no_violations {
        use super::*;

        #[test]
        fn it_prints_nothing() {
            assert_eq!(root_annotations(&report_with(vec![])), "");
        }
    }
}
