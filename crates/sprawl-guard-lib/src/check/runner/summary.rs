use std::collections::BTreeSet;

use crate::classification::ClassifiedSource;

use super::super::directory_fanout::DirectoryFanoutCheck;
use super::super::report::{
    CheckedDirectoryCount, CheckedFileCount, DetectedLanguageCount, ScanSummary,
};

/// Counts the source coverage represented by completed file and directory checks.
pub(super) fn summarize_scan(
    sources: &[ClassifiedSource],
    directory_checks: &[DirectoryFanoutCheck],
) -> ScanSummary {
    let files_checked = CheckedFileCount::from_len(sources.len());
    let directories_checked = CheckedDirectoryCount::from_len(
        directory_checks
            .iter()
            .map(DirectoryFanoutCheck::measurement)
            .map(|measurement| measurement.path)
            .collect::<BTreeSet<_>>()
            .len(),
    );
    let languages_detected = DetectedLanguageCount::from_len(
        sources
            .iter()
            .map(ClassifiedSource::language)
            .collect::<BTreeSet<_>>()
            .len(),
    );
    ScanSummary::new(files_checked, directories_checked, languages_detected)
}
