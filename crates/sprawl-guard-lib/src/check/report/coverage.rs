//! How much of the selected tree a check could analyze.

use serde::Serialize;

use crate::classification::{UnreadablePath, UnreadablePathSeverity};
use crate::config::ParsedCodeFileByteLimit;
use crate::{FileBytes, RelativePath};

use super::FileViolationKind;

/// Whether a check analyzed everything it selected.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalysisCoverage {
    /// Every selected path was read and every source file was measured.
    Complete,
    /// The check finished, but some paths or measurements are missing from it.
    Partial(AnalysisGaps),
}

impl AnalysisCoverage {
    /// Builds the coverage for a run with these gaps, which is complete when both lists are empty.
    pub fn new(unreadable_paths: Vec<UnreadablePath>, uncounted_files: Vec<UncountedFile>) -> Self {
        if unreadable_paths.is_empty() && uncounted_files.is_empty() {
            return Self::Complete;
        }
        Self::Partial(AnalysisGaps {
            unreadable_paths,
            uncounted_files,
        })
    }

    /// Returns the paths below the root that could not be read.
    pub fn unreadable_paths(&self) -> &[UnreadablePath] {
        match self {
            Self::Complete => &[],
            Self::Partial(gaps) => &gaps.unreadable_paths,
        }
    }

    /// Returns the source files whose lines were not counted.
    pub fn uncounted_files(&self) -> &[UncountedFile] {
        match self {
            Self::Complete => &[],
            Self::Partial(gaps) => &gaps.uncounted_files,
        }
    }

    /// Returns whether any gap fails the check under `traversal.unreadable_paths = "fail"`.
    pub(super) fn has_error(&self) -> bool {
        self.unreadable_paths()
            .iter()
            .map(|path| path.severity)
            .chain(self.uncounted_files().iter().map(|file| file.severity))
            .any(|severity| severity == UnreadablePathSeverity::Error)
    }
}

/// What a partial check could not analyze. At least one list is non-empty.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AnalysisGaps {
    unreadable_paths: Vec<UnreadablePath>,
    uncounted_files: Vec<UncountedFile>,
}

/// A source file larger than `traversal.max_parsed_code_file_bytes`, so its lines were not counted.
///
/// The file still exists, so a line ceiling recorded for it in the ratchet file is kept, not
/// reported as stale.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UncountedFile {
    /// Which line limit went unchecked.
    pub kind: FileViolationKind,
    /// Whether this gap fails the check, from `traversal.unreadable_paths`.
    pub severity: UnreadablePathSeverity,
    /// Repository-relative source path.
    pub path: RelativePath,
    /// Size of the file.
    pub size: FileBytes,
    /// Configured `traversal.max_parsed_code_file_bytes`.
    pub limit: ParsedCodeFileByteLimit,
}
