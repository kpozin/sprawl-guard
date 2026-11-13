//! Line counting adapters.

use std::path::Path;

use serde::Serialize;
use tokei::{CodeStats, Config as TokeiConfig};

use crate::{
    CountLines, CountedLines, LanguageId, LineCount, LineCountOverflow, LineCounts, Result,
    SprawlError,
};

const TOKEI_BACKEND_ID: &str = "tokei";
const TOKEI_BACKEND_VERSION: &str = "15.0.0";

/// Counts lines with a specific backend.
pub trait CodeLineCounter {
    /// Measures lines for a source file on disk.
    ///
    /// `language` selects the parser semantics. `count_lines` selects
    /// which recursively summarized line categories contribute to `actual`.
    fn count_path(
        &self,
        path: &Path,
        language: LanguageId,
        count_lines: CountLines,
    ) -> Result<LineMeasurement>;

    /// Measures lines for source text already loaded in memory.
    ///
    /// This is primarily useful for tests and alternative callers that already
    /// own the source contents. The `language` parameter has the same meaning as
    /// in [`CodeLineCounter::count_path`].
    fn count_source(
        &self,
        source: &str,
        language: LanguageId,
        count_lines: CountLines,
    ) -> Result<LineMeasurement>;

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
        count_lines: CountLines,
    ) -> Result<LineMeasurement> {
        let report = language
            .as_tokei()
            .parse(path.to_path_buf(), &TokeiConfig::default())
            .map_err(|(source, path)| SprawlError::ReadSource { path, source })?;
        measure_stats(&report.stats, count_lines, path)
    }

    fn count_source(
        &self,
        source: &str,
        language: LanguageId,
        count_lines: CountLines,
    ) -> Result<LineMeasurement> {
        count_source_text(source, language, count_lines, Path::new("<memory>"))
    }

    fn backend_id(&self) -> &'static str {
        TOKEI_BACKEND_ID
    }

    fn backend_version(&self) -> &'static str {
        TOKEI_BACKEND_VERSION
    }
}

/// One line measurement and its complete line breakdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LineMeasurement {
    /// Line categories selected for the aggregate.
    count_lines: CountLines,
    /// Complete recursively summarized parser counts.
    line_counts: LineCounts,
    /// Sum of the selected line categories.
    actual: CountedLines,
}

impl LineMeasurement {
    /// Creates a measurement with a checked aggregate derived from its breakdown.
    pub fn new(
        count_lines: CountLines,
        line_counts: LineCounts,
    ) -> std::result::Result<Self, LineCountOverflow> {
        Ok(Self {
            count_lines,
            line_counts,
            actual: line_counts.included_total(count_lines)?,
        })
    }

    /// Returns the line categories selected for the aggregate.
    pub const fn count_lines(&self) -> CountLines {
        self.count_lines
    }

    /// Returns the complete recursively summarized parser counts.
    pub const fn line_counts(&self) -> LineCounts {
        self.line_counts
    }

    /// Returns the aggregate of the selected line categories.
    pub const fn actual(&self) -> CountedLines {
        self.actual
    }
}

fn count_source_text(
    source: &str,
    language: LanguageId,
    count_lines: CountLines,
    path: &Path,
) -> Result<LineMeasurement> {
    let stats = language
        .as_tokei()
        .parse_from_str(source, &TokeiConfig::default());
    measure_stats(&stats, count_lines, path)
}

fn measure_stats(
    stats: &CodeStats,
    count_lines: CountLines,
    path: &Path,
) -> Result<LineMeasurement> {
    let summary = stats.summarise();
    let line_counts = LineCounts::new(
        line_count(summary.code, path)?,
        line_count(summary.comments, path)?,
        line_count(summary.blanks, path)?,
    );
    LineMeasurement::new(count_lines, line_counts).map_err(|error| {
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
                .count_source(source, LanguageId::new("Rust").unwrap(), CountLines::Code)
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
                    CountLines::Code,
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
                    CountLines::Code,
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
                    CountLines::CodeAndComments,
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
                .count_source(source, LanguageId::new("Rust").unwrap(), CountLines::Code)
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
                    CountLines::Code,
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
