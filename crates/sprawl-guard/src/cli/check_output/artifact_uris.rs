//! URI references that SARIF results use to name files and directories.

use std::fmt::Write as _;

use serde_json::{Value, json};
use sprawl_guard_lib::RelativePath;

use super::github::FilePaths;

/// How results name files: by URI reference relative to the repository, or not at all.
///
/// SARIF requires `artifactLocation.uri` to be a URI reference, and GitHub code scanning resolves
/// a relative one against the repository, not against the root that was checked.
pub(super) struct ArtifactUris {
    /// Encoded path of the root inside the GitHub workspace, ending in `/`, or empty. `None`
    /// when the root is outside the workspace: a relative URI would then name some other file
    /// in the repository, so results carry no location and their messages name the path.
    prefix: Option<String>,
}

impl ArtifactUris {
    /// Names files as `--format github` annotations name them.
    pub(super) fn new(file_paths: &FilePaths) -> Self {
        Self {
            prefix: match file_paths {
                FilePaths::Prefixed(prefix) => Some(percent_encoded_path(prefix)),
                FilePaths::OutsideWorkspace => None,
            },
        }
    }

    /// Returns the `locations` of a result about a file: the file at its first line, the only
    /// line that applies to a whole-file measurement.
    pub(super) fn file_locations(&self, path: &RelativePath) -> Vec<Value> {
        self.file_physical_location(path)
            .map(|physical_location| json!({ "physicalLocation": physical_location }))
            .into_iter()
            .collect()
    }

    pub(super) fn file_physical_location(&self, path: &RelativePath) -> Option<Value> {
        let mut location = self.artifact_location(path)?;
        location["region"] = json!({ "startLine": 1 });
        Some(location)
    }

    /// Returns the `locations` of a result about a directory, which has no lines.
    pub(super) fn directory_locations(&self, path: &RelativePath) -> Vec<Value> {
        self.artifact_location(path)
            .map(|physical_location| json!({ "physicalLocation": physical_location }))
            .into_iter()
            .collect()
    }

    fn artifact_location(&self, path: &RelativePath) -> Option<Value> {
        let uri = format!(
            "{}{}",
            self.prefix.as_ref()?,
            percent_encoded_path(path.as_str())
        );
        Some(json!({
            "artifactLocation": {
                "uri": if uri.is_empty() { ROOT_DIRECTORY } else { &uri }
            }
        }))
    }
}

/// How a message and a URI name the root directory itself, whose relative path is empty.
pub(super) const ROOT_DIRECTORY: &str = ".";

/// Bytes other than letters and digits that a URI path keeps as they are: the unreserved
/// characters of RFC 3986 and the segment separator.
const UNESCAPED_URI_PATH_BYTES: &[u8] = b"-._~/";

/// Percent-encodes a `/`-separated path for use as a relative URI reference.
///
/// Every byte outside [`UNESCAPED_URI_PATH_BYTES`], letters and digits becomes `%XX`, so
/// non-ASCII text is encoded as its UTF-8 bytes.
fn percent_encoded_path(path: &str) -> String {
    let mut uri = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || UNESCAPED_URI_PATH_BYTES.contains(&byte) {
            uri.push(char::from(byte));
        } else {
            write!(uri, "%{byte:02X}").expect("writing to a string cannot fail");
        }
    }
    uri
}
