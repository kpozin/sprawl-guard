use nutype::nutype;
use serde::Serialize;

/// Number of source files measured during a completed check.
#[nutype(derive(Clone, Copy, Debug, Eq, PartialEq, Serialize))]
pub struct CheckedFileCount(u32);

impl CheckedFileCount {
    pub(crate) fn from_len(value: usize) -> Self {
        Self::new(
            value
                .try_into()
                .expect("checked file count must fit in u32"),
        )
    }
}

/// Number of distinct source directories evaluated during a completed check.
#[nutype(derive(Clone, Copy, Debug, Eq, PartialEq, Serialize))]
pub struct CheckedDirectoryCount(u32);

impl CheckedDirectoryCount {
    pub(crate) fn from_len(value: usize) -> Self {
        Self::new(
            value
                .try_into()
                .expect("checked directory count must fit in u32"),
        )
    }
}

/// Number of languages represented by measured source files.
#[nutype(derive(Clone, Copy, Debug, Eq, PartialEq, Serialize))]
pub struct DetectedLanguageCount(u32);

impl DetectedLanguageCount {
    pub(crate) fn from_len(value: usize) -> Self {
        Self::new(
            value
                .try_into()
                .expect("detected language count must fit in u32"),
        )
    }
}

/// Coverage counts for source files successfully measured by a check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ScanSummary {
    /// Source files measured.
    pub files_checked: CheckedFileCount,
    /// Distinct parent directories represented by measured files.
    pub directories_checked: CheckedDirectoryCount,
    /// Languages represented by measured files.
    pub languages_detected: DetectedLanguageCount,
}

impl ScanSummary {
    /// Creates scan coverage counts.
    pub const fn new(
        files_checked: CheckedFileCount,
        directories_checked: CheckedDirectoryCount,
        languages_detected: DetectedLanguageCount,
    ) -> Self {
        Self {
            files_checked,
            directories_checked,
            languages_detected,
        }
    }
}
