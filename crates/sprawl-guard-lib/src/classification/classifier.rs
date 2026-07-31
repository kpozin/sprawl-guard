use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::config::Config;
use crate::{GlobPattern, LanguageId, RelativePath, Result, SprawlError};

use super::types::{ClassifiedSource, PathClassification, SourceExplanation, SourceKind};

/// Decision engine for files that are visible to ignore-aware traversal.
///
/// The classifier answers whether a repository-relative path should become a
/// checked source file. Included files are then tagged with a language and with
/// either `SourceKind::Production` or `SourceKind::Test`. It deliberately does
/// not read the filesystem; callers provide paths that traversal or `explain`
/// has already normalized under the repository root.
pub(super) struct Classifier<'config> {
    config: &'config Config,
    global_exclude: GlobSet,
    global_tests: GlobSet,
    languages: Vec<LanguageMatcher>,
}

impl<'config> Classifier<'config> {
    /// Builds the matcher state needed to classify visible paths.
    ///
    /// Output is either a classifier ready for repeated path checks or the glob
    /// parse error that makes classification impossible.
    pub(super) fn new(config: &'config Config) -> Result<Self> {
        Ok(Self {
            config,
            global_exclude: glob_set(&config.files.exclude)?,
            global_tests: glob_set(&config.tests.globs)?,
            languages: language_matchers(config)?,
        })
    }

    /// Converts a diagnostic classification into discovery output.
    ///
    /// `Some(ClassifiedSource)` means the path should be checked. `None` means
    /// the path was intentionally left out by configured policy. Ambiguous
    /// language matches become errors because later checks need one definitive
    /// language-specific policy.
    pub(super) fn included_source(&self, path: RelativePath) -> Result<Option<ClassifiedSource>> {
        match self.classify(path)? {
            PathClassification::Source(explanation) => Ok(Some(explanation.source)),
            PathClassification::AmbiguousLanguage { path, languages } => {
                Err(ambiguous_language(path, languages))
            }
            PathClassification::Ignored { .. }
            | PathClassification::GloballyExcluded { .. }
            | PathClassification::NonFile { .. }
            | PathClassification::NotSource { .. }
            | PathClassification::LanguageExcluded { .. }
            | PathClassification::LanguagesExcluded { .. }
            | PathClassification::TestExcluded { .. } => Ok(None),
        }
    }

    /// Explains the first policy decision that applies to one visible path.
    ///
    /// The returned variant is the reason discovery would include or exclude
    /// the path: global exclude, no language match, language-level exclusion,
    /// all matching languages excluded, ambiguous language match, test file
    /// excluded because path-based test checks are disabled, or included source
    /// file. Ignore decisions are not produced here because callers must run
    /// ignore-aware traversal before calling this method.
    pub(super) fn classify(&self, path: RelativePath) -> Result<PathClassification> {
        if self.global_exclude.is_match(path.as_str()) {
            return Ok(PathClassification::GloballyExcluded { path });
        }

        let included_languages = self
            .languages
            .iter()
            .filter(|language| language.include.is_match(path.as_str()))
            .collect::<Vec<_>>();

        if included_languages.is_empty() {
            return Ok(PathClassification::NotSource { path });
        }

        let matches = included_languages
            .iter()
            .copied()
            .filter(|language| !language.exclude.is_match(path.as_str()))
            .collect::<Vec<_>>();

        if matches.is_empty() {
            let languages = included_languages
                .iter()
                .map(|language| language.id)
                .collect::<Vec<_>>();
            return match languages.as_slice() {
                [language] => Ok(PathClassification::LanguageExcluded {
                    path,
                    language: *language,
                }),
                _ => Ok(PathClassification::LanguagesExcluded { path, languages }),
            };
        }

        let [language] = matches.as_slice() else {
            return Ok(PathClassification::AmbiguousLanguage {
                path,
                languages: matches.iter().map(|language| language.id).collect(),
            });
        };

        let is_test = self.global_tests.is_match(path.as_str())
            || language.test_globs.is_match(path.as_str());
        if is_test && !self.config.tests.check_path_tests {
            return Ok(PathClassification::TestExcluded {
                path,
                language: language.id,
            });
        }

        let kind = if is_test {
            SourceKind::Test
        } else {
            SourceKind::Production
        };
        Ok(PathClassification::Source(SourceExplanation::classified(
            ClassifiedSource::new(path, language.id, kind),
        )))
    }
}

/// Compiled include, exclude, and test-classification globs for one language.
struct LanguageMatcher {
    id: LanguageId,
    include: GlobSet,
    exclude: GlobSet,
    test_globs: GlobSet,
}

/// Builds matchers for enabled languages only.
///
/// Disabled languages produce no output, so their include patterns cannot make a
/// path ambiguous and their test globs cannot mark a source file as test code.
fn language_matchers(config: &Config) -> Result<Vec<LanguageMatcher>> {
    config
        .languages
        .iter()
        .filter(|(_, language)| language.is_enabled())
        .map(|(id, language)| {
            let common = language.common();
            Ok(LanguageMatcher {
                id: *id,
                include: glob_set(&common.include)?,
                exclude: glob_set(&common.exclude)?,
                test_globs: glob_set(&common.test_globs)?,
            })
        })
        .collect()
}

/// Compiles zero or more config globs into one matcher set.
///
/// An empty input produces a matcher that never matches. Invalid input produces
/// `SprawlError::InvalidGlob` with the failing pattern when available.
fn glob_set(patterns: &[GlobPattern]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            Glob::new(pattern.as_str()).map_err(|source| SprawlError::InvalidGlob {
                pattern: pattern.to_string(),
                source,
            })?,
        );
    }
    builder.build().map_err(|source| SprawlError::InvalidGlob {
        pattern: "<glob set>".to_owned(),
        source,
    })
}

/// Converts an ambiguous diagnostic result into the public discovery error.
///
/// Discovery reports ambiguity as an error, while `explain` can return the
/// structured `PathClassification::AmbiguousLanguage` value directly.
fn ambiguous_language(path: RelativePath, languages: Vec<LanguageId>) -> SprawlError {
    SprawlError::AmbiguousLanguage { path, languages }
}
