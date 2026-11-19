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

/// Number of entries in the ratchet file: file ceilings plus directory ceilings.
#[nutype(derive(Clone, Copy, Debug, Eq, PartialEq, Serialize))]
pub struct RatchetEntryCount(u32);

impl RatchetEntryCount {
    pub(crate) fn from_len(value: usize) -> Self {
        Self::new(
            value
                .try_into()
                .expect("ratchet entry count must fit in u32"),
        )
    }
}

/// Counts reported with a completed check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ScanSummary {
    /// Source files measured.
    pub files_checked: CheckedFileCount,
    /// Distinct directories whose direct files were checked. A directory with only some of its
    /// files selected is not one of them.
    pub directories_checked: CheckedDirectoryCount,
    /// Languages represented by measured files.
    pub languages_detected: DetectedLanguageCount,
    /// Entries in the ratchet file, counted across the whole repository rather than the
    /// requested paths. Zero when no ratchet file exists.
    pub ratchet_entries: RatchetEntryCount,
}

impl ScanSummary {
    /// Creates scan coverage counts.
    pub const fn new(
        files_checked: CheckedFileCount,
        directories_checked: CheckedDirectoryCount,
        languages_detected: DetectedLanguageCount,
        ratchet_entries: RatchetEntryCount,
    ) -> Self {
        Self {
            files_checked,
            directories_checked,
            languages_detected,
            ratchet_entries,
        }
    }
}
