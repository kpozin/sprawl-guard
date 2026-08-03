use std::collections::BTreeMap;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::config::Config;
use crate::{GlobPattern, LanguageId, RelativePath, Result, SprawlError};

use super::bundled_languages::BundledLanguage;
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
    languages: LanguageMatchers,
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
            | PathClassification::RatchetExcluded { .. }
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
        if path.as_str() == self.config.ratchet.file.as_str() {
            return Ok(PathClassification::RatchetExcluded { path });
        }
        if self.global_exclude.is_match(path.as_str()) {
            return Ok(PathClassification::GloballyExcluded { path });
        }

        let bundled_owner =
            BundledLanguage::from_path(Path::new(path.as_str())).map(BundledLanguage::as_tokei);
        let included_languages = self.languages.matching(path.as_str(), bundled_owner);

        if included_languages.is_empty() {
            return Ok(PathClassification::NotSource { path });
        }

        let matches = included_languages
            .iter()
            .copied()
            .filter(|language| !is_match(&language.exclude, path.as_str()))
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
            || is_match(&language.test_globs, path.as_str());
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
    authored_include: Option<GlobSet>,
    exclude: Option<GlobSet>,
    test_globs: Option<GlobSet>,
}

/// Enabled language matchers indexed by their two candidate-selection paths.
struct LanguageMatchers {
    entries: Vec<LanguageMatcher>,
    bundled: BTreeMap<LanguageId, usize>,
    authored: Vec<usize>,
}

impl LanguageMatchers {
    fn matching(
        &self,
        path: &str,
        bundled_owner: Option<tokei::LanguageType>,
    ) -> Vec<&LanguageMatcher> {
        let bundled_index = bundled_owner
            .map(LanguageId::from_tokei)
            .and_then(|language| self.bundled.get(&language).copied());
        let mut matching_indices = bundled_index.map(|index| vec![index]).unwrap_or_default();

        matching_indices.extend(self.authored.iter().filter_map(|index| {
            if Some(*index) == bundled_index {
                return None;
            }
            let matcher = &self.entries[*index];
            is_match(&matcher.authored_include, path).then_some(*index)
        }));
        matching_indices.sort_unstable();
        matching_indices
            .into_iter()
            .map(|index| &self.entries[index])
            .collect()
    }
}

/// Builds matchers for enabled languages only.
///
/// Disabled languages produce no output, so their include patterns cannot make a
/// path ambiguous and their test globs cannot mark a source file as test code.
fn language_matchers(config: &Config) -> Result<LanguageMatchers> {
    let mut entries = Vec::new();
    let mut bundled = BTreeMap::new();
    let mut authored = Vec::new();

    for (id, language) in config
        .languages
        .iter()
        .filter(|(_, language)| language.is_enabled())
    {
        let common = language.common();
        let authored_include = optional_glob_set(
            common
                .include
                .iter()
                .flatten()
                .chain(&common.additional_globs),
        )?;
        let index = entries.len();
        if common.include.is_none() {
            bundled.insert(*id, index);
        }
        if authored_include.is_some() {
            authored.push(index);
        }
        entries.push(LanguageMatcher {
            id: *id,
            authored_include,
            exclude: optional_glob_set(&common.exclude)?,
            test_globs: optional_glob_set(&common.test_globs)?,
        });
    }

    Ok(LanguageMatchers {
        entries,
        bundled,
        authored,
    })
}

fn is_match(globs: &Option<GlobSet>, path: &str) -> bool {
    globs.as_ref().is_some_and(|globs| globs.is_match(path))
}

/// Compiles authored globs when at least one pattern exists.
fn optional_glob_set<'pattern>(
    patterns: impl IntoIterator<Item = &'pattern GlobPattern>,
) -> Result<Option<GlobSet>> {
    let mut builder = GlobSetBuilder::new();
    let mut has_patterns = false;
    for pattern in patterns {
        has_patterns = true;
        builder.add(
            Glob::new(pattern.as_str()).map_err(|source| SprawlError::InvalidGlob {
                pattern: pattern.to_string(),
                source,
            })?,
        );
    }
    if !has_patterns {
        return Ok(None);
    }
    builder
        .build()
        .map(Some)
        .map_err(|source| SprawlError::InvalidGlob {
            pattern: "<glob set>".to_owned(),
            source,
        })
}

/// Compiles zero or more config globs into one matcher set.
///
/// An empty input produces a matcher that never matches. Invalid input produces
/// `SprawlError::InvalidGlob` with the failing pattern when available.
fn glob_set<'pattern>(
    patterns: impl IntoIterator<Item = &'pattern GlobPattern>,
) -> Result<GlobSet> {
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
