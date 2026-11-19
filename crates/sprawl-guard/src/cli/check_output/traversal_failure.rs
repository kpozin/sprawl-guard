//! Reports for a traversal that could not read enough of the tree to produce a check report.

use std::path::Path;

use sprawl_guard_lib::check::CheckErrorKind;
use sprawl_guard_lib::{ConfigSource, FiniteOrUnlimited, MaxFindings, UnreadablePath};

use super::listing::entry_limit;
use super::{
    config_source_notice, github, unlisted_message, unreadable_path_message,
    write_truncation_footer,
};
use crate::error::{CliError, Result};

fn traversal_failure_json_value(
    root: &Path,
    config_source: &ConfigSource,
    unreadable_paths: &[UnreadablePath],
) -> serde_json::Value {
    serde_json::json!({
        "status": "error",
        "root": root.display().to_string(),
        "config_source": config_source,
        "coverage": {
            "kind": "partial",
            "unreadable_paths": unreadable_paths,
        },
        "warnings": [],
        "error": {
            "kind": CheckErrorKind::TraversalFailed,
            "message": format!("could not read {} path(s)", unreadable_paths.len()),
        },
        "violations": [],
    })
}

pub(in crate::cli) fn render_traversal_failure_json(
    root: &Path,
    config_source: &ConfigSource,
    unreadable_paths: &[UnreadablePath],
) -> Result<String> {
    serde_json::to_string_pretty(&traversal_failure_json_value(
        root,
        config_source,
        unreadable_paths,
    ))
    .map(|output| format!("{output}\n"))
    .map_err(|source| CliError::RenderJson { source })
}

/// Renders the traversal failure of a command without a finding cap, listing every path.
pub(in crate::cli) fn render_traversal_failure_human(
    root: &Path,
    unreadable_paths: &[UnreadablePath],
) -> String {
    traversal_failure_human(root, unreadable_paths, FiniteOrUnlimited::Unlimited)
}

/// Renders the traversal failure of `check`, listing unreadable paths up to the finding cap.
pub(in crate::cli) fn render_check_traversal_failure_human(
    root: &Path,
    config_source: &ConfigSource,
    unreadable_paths: &[UnreadablePath],
    max_findings: FiniteOrUnlimited<MaxFindings>,
) -> String {
    let mut output = config_source_notice(config_source, root);
    output.push_str(&traversal_failure_human(
        root,
        unreadable_paths,
        max_findings,
    ));
    output
}

/// Renders the traversal failure report for GitHub output, as plain log text on stderr.
pub(in crate::cli) fn render_check_traversal_failure_github(
    root: &Path,
    config_source: &ConfigSource,
    unreadable_paths: &[UnreadablePath],
    max_findings: FiniteOrUnlimited<MaxFindings>,
) -> String {
    github::plain_log_text(&render_check_traversal_failure_human(
        root,
        config_source,
        unreadable_paths,
        max_findings,
    ))
}

fn traversal_failure_human(
    root: &Path,
    unreadable_paths: &[UnreadablePath],
    max_findings: FiniteOrUnlimited<MaxFindings>,
) -> String {
    let total = unreadable_paths.len();
    let listed = total.min(entry_limit(max_findings));
    let mut output = format!(
        "Could not read {total} path(s) under {}:\n\n",
        root.display()
    );
    for unreadable_path in &unreadable_paths[..listed] {
        output.push_str("  ");
        output.push_str(&unreadable_path_message(unreadable_path));
        output.push('\n');
    }
    output.extend(unlisted_message(total, listed));
    write_truncation_footer(&mut output, listed, total);
    output
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use sprawl_guard_lib::{
        DiagnosticDetail, RelativePath, UnreadablePathKind, UnreadablePathSeverity,
    };

    use super::*;

    fn unreadable_path(path: &str) -> UnreadablePath {
        UnreadablePath {
            kind: UnreadablePathKind::ReadDirFailed,
            severity: UnreadablePathSeverity::Error,
            path: Some(RelativePath::new(path).unwrap()),
            detail: DiagnosticDetail::new("permission denied"),
        }
    }

    fn render(max_findings: FiniteOrUnlimited<MaxFindings>) -> String {
        render_check_traversal_failure_human(
            Path::new("/repo"),
            &ConfigSource::BuiltIn,
            &[unreadable_path("generated"), unreadable_path("vendor")],
            max_findings,
        )
    }

    mod when_unreadable_paths_exceed_the_cap {
        use super::*;

        #[test]
        fn it_lists_up_to_the_cap_and_keeps_the_total() {
            let output = render(FiniteOrUnlimited::Finite(MaxFindings::new(1).unwrap()));

            assert!(output.ends_with(indoc! {"
                Could not read 2 path(s) under /repo:

                  failed to read directory at generated: permission denied
                  … and 1 more

                Showing 1 of 2 findings. Use --max-findings unlimited to see all.
            "}));
        }
    }

    mod when_the_cap_is_unlimited {
        use super::*;

        #[test]
        fn it_lists_every_path_without_a_footer() {
            let output = render(FiniteOrUnlimited::Unlimited);

            assert!(output.ends_with(concat!(
                "  failed to read directory at generated: permission denied\n",
                "  failed to read directory at vendor: permission denied\n",
            )));
            assert!(!output.contains("Showing"));
        }
    }
}
