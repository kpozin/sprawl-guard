//! The message a ratchet command prints when it refuses to write after an incomplete traversal.

use std::path::Path;

use sprawl_guard_lib::UnreadablePath;

use super::super::check_output::unreadable_path_message;

/// Lists every path below `root` that could not be read.
pub(in crate::cli) fn render_refusal(root: &Path, unreadable_paths: &[UnreadablePath]) -> String {
    let mut output = format!(
        "Could not read {} path(s) under {}:\n\n",
        unreadable_paths.len(),
        root.display()
    );
    for unreadable_path in unreadable_paths {
        output.push_str("  ");
        output.push_str(&unreadable_path_message(unreadable_path));
        output.push('\n');
    }
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

    mod when_paths_are_unreadable {
        use super::*;

        #[test]
        fn it_lists_every_path() {
            let output = render_refusal(
                Path::new("/repo"),
                &[unreadable_path("generated"), unreadable_path("vendor")],
            );

            assert_eq!(
                output,
                indoc! {"
                    Could not read 2 path(s) under /repo:

                      failed to read directory at generated: permission denied
                      failed to read directory at vendor: permission denied
                "}
            );
        }
    }
}
