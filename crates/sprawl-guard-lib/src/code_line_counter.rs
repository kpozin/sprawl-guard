//! Source code-line counting adapters.

use std::path::Path;

use tokei::{CodeStats, Config as TokeiConfig};

use crate::{
    CountedLoc, IncludedLineTypes, LanguageId, LineCount, LineCountOverflow, LineCounts, Result,
    SprawlError,
};

const TOKEI_BACKEND_ID: &str = "tokei";
const TOKEI_BACKEND_VERSION: &str = "14.0.0";

/// Counts source code lines with a specific backend.
pub trait CodeLineCounter {
    /// Measures logical lines for a source file on disk.
    ///
    /// `language` selects the parser semantics. `included_line_types` selects
    /// which recursively summarized line categories contribute to `actual`.
    fn count_path(
        &self,
        path: &Path,
        language: LanguageId,
        included_line_types: IncludedLineTypes,
    ) -> Result<LocMeasurement>;

    /// Measures logical lines for source text already loaded in memory.
    ///
    /// This is primarily useful for tests and alternative callers that already
    /// own the source contents. The `language` parameter has the same meaning as
    /// in [`CodeLineCounter::count_path`].
    fn count_source(
        &self,
        source: &str,
        language: LanguageId,
        included_line_types: IncludedLineTypes,
    ) -> Result<LocMeasurement>;

    /// Stable backend identifier copied into `CheckReport.counter.backend`.
    fn backend_id(&self) -> &'static str;

    /// Stable backend version copied into `CheckReport.counter.version`.
    fn backend_version(&self) -> &'static str;
}

/// Code-line counter backed by `tokei` parser statistics.
#[derive(Clone, Copy, Debug, Default)]
pub struct TokeiCodeLineCounter;

impl CodeLineCounter for TokeiCodeLineCounter {
    fn count_path(
        &self,
        path: &Path,
        language: LanguageId,
        included_line_types: IncludedLineTypes,
    ) -> Result<LocMeasurement> {
        let report = language
            .as_tokei()
            .parse(path.to_path_buf(), &TokeiConfig::default())
            .map_err(|(source, path)| SprawlError::ReadSource { path, source })?;
        measure_stats(&report.stats, included_line_types, path)
    }

    fn count_source(
        &self,
        source: &str,
        language: LanguageId,
        included_line_types: IncludedLineTypes,
    ) -> Result<LocMeasurement> {
        count_source_text(source, language, included_line_types, Path::new("<memory>"))
    }

    fn backend_id(&self) -> &'static str {
        TOKEI_BACKEND_ID
    }

    fn backend_version(&self) -> &'static str {
        TOKEI_BACKEND_VERSION
    }
}

/// One configured LOC measurement and its complete logical-line breakdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocMeasurement {
    /// Line categories selected for the aggregate.
    included_line_types: IncludedLineTypes,
    /// Complete recursively summarized parser counts.
    line_counts: LineCounts,
    /// Sum of the selected line categories.
    actual: CountedLoc,
}

impl LocMeasurement {
    /// Creates a measurement with a checked aggregate derived from its breakdown.
    pub fn new(
        included_line_types: IncludedLineTypes,
        line_counts: LineCounts,
    ) -> std::result::Result<Self, LineCountOverflow> {
        Ok(Self {
            included_line_types,
            line_counts,
            actual: line_counts.included_total(included_line_types)?,
        })
    }

    /// Returns the line categories selected for the aggregate.
    pub const fn included_line_types(&self) -> IncludedLineTypes {
        self.included_line_types
    }

    /// Returns the complete recursively summarized parser counts.
    pub const fn line_counts(&self) -> LineCounts {
        self.line_counts
    }

    /// Returns the aggregate of the selected line categories.
    pub const fn actual(&self) -> CountedLoc {
        self.actual
    }
}

fn count_source_text(
    source: &str,
    language: LanguageId,
    included_line_types: IncludedLineTypes,
    path: &Path,
) -> Result<LocMeasurement> {
    let stats = language
        .as_tokei()
        .parse_from_str(source, &TokeiConfig::default());
    measure_stats(&stats, included_line_types, path)
}

fn measure_stats(
    stats: &CodeStats,
    included_line_types: IncludedLineTypes,
    path: &Path,
) -> Result<LocMeasurement> {
    let summary = stats.summarise();
    let line_counts = LineCounts::new(
        line_count(summary.code, path)?,
        line_count(summary.comments, path)?,
        line_count(summary.blanks, path)?,
    );
    LocMeasurement::new(included_line_types, line_counts).map_err(|error| {
        SprawlError::CodeLineCountOverflow {
            path: path.to_path_buf(),
            count: error.count(),
        }
    })
}

fn line_count(count: usize, path: &Path) -> Result<LineCount> {
    let count = u32::try_from(count).map_err(|_| SprawlError::CodeLineCountOverflow {
        path: path.to_path_buf(),
        count: count as u64,
    })?;
    Ok(LineCount::new(count))
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    mod when_counting_rust_source {
        use super::*;

        #[test]
        fn it_excludes_comments_and_blank_lines_using_tokei_semantics() {
            let source = indoc! {r#"
                fn main() {
                    // comment
                    let value = 1;

                    println!("{value}");
                }
            "#};

            let lines = TokeiCodeLineCounter
                .count_source(
                    source,
                    LanguageId::new("Rust").unwrap(),
                    IncludedLineTypes::Code,
                )
                .unwrap();

            assert_eq!(lines.actual().get(), 4);
        }
    }

    mod when_counting_javascript_source {
        use super::*;

        #[test]
        fn it_counts_a_representative_non_rust_fixture() {
            let source = indoc! {r#"
                export function value() {
                  return 1;
                }
            "#};

            let lines = TokeiCodeLineCounter
                .count_source(
                    source,
                    LanguageId::new("JavaScript").unwrap(),
                    IncludedLineTypes::Code,
                )
                .unwrap();

            assert_eq!(lines.actual().get(), 3);
        }
    }

    mod when_counting_source_from_disk {
        use super::*;

        #[test]
        fn it_uses_tokeis_native_decoding() {
            let tempdir = tempfile::tempdir().unwrap();
            let path = tempdir.path().join("latin1.js");
            std::fs::write(&path, b"const label = '\xE9';\n").unwrap();

            let lines = TokeiCodeLineCounter
                .count_path(
                    &path,
                    LanguageId::new("JavaScript").unwrap(),
                    IncludedLineTypes::Code,
                )
                .unwrap();

            assert_eq!(lines.actual().get(), 1);
        }
    }

    mod when_counting_the_markdown_golden_fixture {
        use super::*;

        #[test]
        fn it_recursively_summarizes_fenced_rust() {
            let source = include_str!("../testdata/line_counts/markdown.md");

            let measurement = TokeiCodeLineCounter
                .count_source(
                    source,
                    LanguageId::new("Markdown").unwrap(),
                    IncludedLineTypes::CodeAndComments,
                )
                .unwrap();

            assert_eq!(
                measurement.line_counts(),
                LineCounts::new(LineCount::new(1), LineCount::new(3), LineCount::new(1)),
            );
            assert_eq!(measurement.actual().get(), 4);
        }
    }

    mod when_counting_the_rust_doc_comment_golden_fixture {
        use super::*;

        #[test]
        fn it_recursively_summarizes_markdown_and_fenced_rust() {
            let source = include_str!("../testdata/line_counts/rust_doc_comment.rs");

            let measurement = TokeiCodeLineCounter
                .count_source(
                    source,
                    LanguageId::new("Rust").unwrap(),
                    IncludedLineTypes::Code,
                )
                .unwrap();

            assert_eq!(
                measurement.line_counts(),
                LineCounts::new(LineCount::new(2), LineCount::new(3), LineCount::new(0)),
            );
            assert_eq!(measurement.actual().get(), 2);
        }
    }

    mod when_counting_inline_html_embedding {
        use super::*;

        #[test]
        fn it_keeps_parent_delimiters_and_child_code_on_the_same_physical_line() {
            let measurement = TokeiCodeLineCounter
                .count_source(
                    "<script>const value = 1;</script>",
                    LanguageId::new("Html").unwrap(),
                    IncludedLineTypes::Code,
                )
                .unwrap();

            assert_eq!(
                measurement.line_counts(),
                LineCounts::new(LineCount::new(3), LineCount::new(0), LineCount::new(0)),
            );
            assert_eq!(measurement.actual().get(), 3);
        }
    }
}
