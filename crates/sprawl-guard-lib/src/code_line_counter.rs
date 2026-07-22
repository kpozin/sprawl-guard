//! Source code-line counting adapters.

use std::path::Path;

use tokei::Config as TokeiConfig;

use crate::{CountedCodeLines, LanguageId, Result, SprawlError};

const TOKEI_BACKEND_ID: &str = "tokei";
const TOKEI_BACKEND_VERSION: &str = "14.0.0";

/// Counts source code lines with a specific backend.
pub trait CodeLineCounter {
    /// Counts code lines for a source file on disk.
    ///
    /// `language` selects the parser semantics. The returned count excludes
    /// comments and blank lines according to the backend.
    fn count_path(&self, path: &Path, language: LanguageId) -> Result<CountedCodeLines>;

    /// Counts code lines for source text already loaded in memory.
    ///
    /// This is primarily useful for tests and alternative callers that already
    /// own the source contents. The `language` parameter has the same meaning as
    /// in [`CodeLineCounter::count_path`].
    fn count_source(&self, source: &str, language: LanguageId) -> Result<CountedCodeLines>;

    /// Stable backend identifier copied into `CheckReport.counter.backend`.
    fn backend_id(&self) -> &'static str;

    /// Stable backend version copied into `CheckReport.counter.version`.
    fn backend_version(&self) -> &'static str;
}

/// Code-line counter backed by `tokei` parser statistics.
#[derive(Clone, Copy, Debug, Default)]
pub struct TokeiCodeLineCounter;

impl CodeLineCounter for TokeiCodeLineCounter {
    fn count_path(&self, path: &Path, language: LanguageId) -> Result<CountedCodeLines> {
        let report = language
            .as_tokei()
            .parse(path.to_path_buf(), &TokeiConfig::default())
            .map_err(|(source, path)| SprawlError::ReadSource { path, source })?;
        count_code_lines(report.stats.code, path)
    }

    fn count_source(&self, source: &str, language: LanguageId) -> Result<CountedCodeLines> {
        count_source_text(source, language, Path::new("<memory>"))
    }

    fn backend_id(&self) -> &'static str {
        TOKEI_BACKEND_ID
    }

    fn backend_version(&self) -> &'static str {
        TOKEI_BACKEND_VERSION
    }
}

fn count_source_text(source: &str, language: LanguageId, path: &Path) -> Result<CountedCodeLines> {
    let stats = language
        .as_tokei()
        .parse_from_str(source, &TokeiConfig::default());
    count_code_lines(stats.code, path)
}

fn count_code_lines(count: usize, path: &Path) -> Result<CountedCodeLines> {
    let count = u32::try_from(count).map_err(|_| SprawlError::CodeLineCountOverflow {
        path: path.to_path_buf(),
        count,
    })?;
    Ok(CountedCodeLines::new(count))
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
                .count_source(source, LanguageId::new("Rust").unwrap())
                .unwrap();

            assert_eq!(lines.get(), 4);
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
                .count_source(source, LanguageId::new("JavaScript").unwrap())
                .unwrap();

            assert_eq!(lines.get(), 3);
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
                .count_path(&path, LanguageId::new("JavaScript").unwrap())
                .unwrap();

            assert_eq!(lines.get(), 1);
        }
    }
}
