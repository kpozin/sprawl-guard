//! Resolved policy limits after applying language and path overrides.

use std::num::NonZeroUsize;
use std::path::Path;

use globset::{Glob, GlobMatcher};
use serde::Serialize;

use crate::classification::{ClassifiedSource, SourceKind};
use crate::config::{Config, OverrideLimits, OverridePaths, OverrideReason, TestFanoutLimit};
use crate::{CodeLines, LeafFiles, RelativePath, Result, SprawlError};

const DIRECTORY_OVERRIDE_DESCENDANT_PROBE: &str = "__sprawl_guard_directory_metric__";

/// One-based position of a matching `[[overrides]]` entry in declaration order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct OverrideOrdinal(NonZeroUsize);

impl OverrideOrdinal {
    /// Creates an ordinal from a zero-based override index.
    fn from_zero_based(index: usize) -> Self {
        let ordinal = index
            .checked_add(1)
            .and_then(NonZeroUsize::new)
            .expect("override ordinal must fit in usize and be nonzero");
        Self(ordinal)
    }

    /// Returns the one-based override position.
    pub const fn get(self) -> usize {
        self.0.get()
    }
}

/// A path override that contributed to a resolved metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MatchedOverride {
    /// One-based position of the override in the resolved config.
    pub ordinal: OverrideOrdinal,
    /// Glob patterns from the matching override.
    pub paths: OverridePaths,
    /// Limits set by the matching override.
    pub limits: OverrideLimits,
    /// Optional rationale from the matching override.
    pub reason: Option<OverrideReason>,
}

/// Resolved code-line limit for one file metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedCodeLineLimit {
    /// Final code-line limit after precedence is applied.
    pub value: CodeLines,
    /// Overrides whose path globs matched the file path.
    pub matched_overrides: Vec<MatchedOverride>,
}

/// Resolved leaf-file limit for one directory metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedLeafFileLimit {
    /// Final direct leaf-file limit after precedence is applied.
    pub value: LeafFiles,
    /// Overrides whose path globs matched the directory metric path.
    ///
    /// A trailing `/**` pattern also matches the prefix directory itself for
    /// directory metrics. For example, `src/**` matches both `src` and
    /// `src/generated`.
    pub matched_overrides: Vec<MatchedOverride>,
}

/// Resolved file and directory limits for an included source path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceResolvedLimits {
    /// Resolved file LOC limit for the source's production/test kind.
    pub file: ResolvedCodeLineLimit,
    /// Resolved directory fan-out limit for the source's parent directory, when finite.
    pub directory: Option<ResolvedLeafFileLimit>,
}

/// Resolves configured limits for concrete file and directory metrics.
pub(crate) struct LimitResolver<'config> {
    config: &'config Config,
    overrides: Vec<CompiledOverride<'config>>,
}

impl<'config> LimitResolver<'config> {
    /// Compiles override path globs once for repeated metric resolution.
    pub(crate) fn new(config: &'config Config) -> Result<Self> {
        Ok(Self {
            config,
            overrides: config
                .overrides
                .iter()
                .enumerate()
                .map(|(index, override_entry)| {
                    Ok(CompiledOverride {
                        ordinal: OverrideOrdinal::from_zero_based(index),
                        paths: override_path_matchers(&override_entry.paths)?,
                        limits: &override_entry.limits,
                        original_paths: &override_entry.paths,
                        reason: override_entry.reason.as_ref(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }

    /// Resolves both file and parent-directory limits for one included source.
    pub(crate) fn source_limits(&self, source: &ClassifiedSource) -> Result<SourceResolvedLimits> {
        Ok(SourceResolvedLimits {
            file: self.file_code_lines(source),
            directory: self
                .directory_leaf_files(source.kind, &source_parent_directory(&source.path)?),
        })
    }

    /// Resolves the file LOC limit for one included source.
    pub(crate) fn file_code_lines(&self, source: &ClassifiedSource) -> ResolvedCodeLineLimit {
        let mut limit = match source.kind {
            SourceKind::Production => self.production_file_code_lines(source),
            SourceKind::Test => self.config.limits.max_lines_per_file.code,
        };
        let mut matched_overrides = Vec::new();

        for override_entry in &self.overrides {
            if !file_matches_override(&override_entry.paths, &source.path) {
                continue;
            }
            matched_overrides.push(override_entry.matched_override());
            match source.kind {
                SourceKind::Production => {
                    if let Some(value) = override_entry.limits.code_non_test_lines_per_file() {
                        limit = value;
                    }
                }
                SourceKind::Test => {
                    if let Some(value) = override_entry.limits.code_lines_per_test_file() {
                        limit = value;
                    }
                }
            }
        }

        ResolvedCodeLineLimit {
            value: limit,
            matched_overrides,
        }
    }

    /// Resolves the direct leaf-file limit for one directory metric.
    pub(crate) fn directory_leaf_files(
        &self,
        kind: SourceKind,
        path: &RelativePath,
    ) -> Option<ResolvedLeafFileLimit> {
        let mut limit = match kind {
            SourceKind::Production => {
                Some(self.config.limits.max_leaf_files_per_directory.production)
            }
            SourceKind::Test => {
                finite_test_fanout_limit(self.config.limits.max_leaf_files_per_directory.test)
            }
        };
        let mut matched_overrides = Vec::new();

        for override_entry in &self.overrides {
            if !directory_matches_override(&override_entry.paths, path) {
                continue;
            }
            matched_overrides.push(override_entry.matched_override());
            match kind {
                SourceKind::Production => {
                    if let Some(value) = override_entry.limits.production_leaf_files_per_directory()
                    {
                        limit = Some(value);
                    }
                }
                SourceKind::Test => {
                    if let Some(value) = override_entry.limits.test_leaf_files_per_directory() {
                        limit = finite_test_fanout_limit(value);
                    }
                }
            }
        }

        Some(ResolvedLeafFileLimit {
            value: limit?,
            matched_overrides,
        })
    }

    fn production_file_code_lines(&self, source: &ClassifiedSource) -> CodeLines {
        self.config
            .languages()
            .get(&source.language)
            .and_then(|language| language.common().limits.max_lines_per_file.code_non_test)
            .unwrap_or(self.config.limits.max_lines_per_file.code_non_test)
    }
}

fn finite_test_fanout_limit(limit: TestFanoutLimit) -> Option<LeafFiles> {
    match limit {
        TestFanoutLimit::Finite(value) => Some(value),
        TestFanoutLimit::Unlimited => None,
    }
}

struct CompiledOverride<'config> {
    ordinal: OverrideOrdinal,
    paths: Vec<OverridePathMatcher>,
    limits: &'config OverrideLimits,
    original_paths: &'config OverridePaths,
    reason: Option<&'config OverrideReason>,
}

impl CompiledOverride<'_> {
    fn matched_override(&self) -> MatchedOverride {
        MatchedOverride {
            ordinal: self.ordinal,
            paths: self.original_paths.clone(),
            limits: self.limits.clone(),
            reason: self.reason.cloned(),
        }
    }
}

struct OverridePathMatcher {
    matches_directory_descendants: bool,
    matcher: GlobMatcher,
}

fn override_path_matchers(paths: &OverridePaths) -> Result<Vec<OverridePathMatcher>> {
    paths
        .as_slice()
        .iter()
        .map(|path| {
            let glob = Glob::new(path.as_str()).map_err(|source| SprawlError::InvalidGlob {
                pattern: path.to_string(),
                source,
            })?;
            Ok(OverridePathMatcher {
                matches_directory_descendants: path.as_str().ends_with("/**"),
                matcher: glob.compile_matcher(),
            })
        })
        .collect()
}

fn file_matches_override(matchers: &[OverridePathMatcher], path: &RelativePath) -> bool {
    matchers
        .iter()
        .any(|matcher| matcher.matcher.is_match(path.as_str()))
}

fn directory_matches_override(matchers: &[OverridePathMatcher], path: &RelativePath) -> bool {
    matchers
        .iter()
        .any(|matcher| directory_path_matches_override(matcher, path))
}

fn directory_path_matches_override(matcher: &OverridePathMatcher, path: &RelativePath) -> bool {
    matcher.matcher.is_match(path.as_str())
        || (matcher.matches_directory_descendants
            && matcher.matcher.is_match(directory_descendant_probe(path)))
}

fn directory_descendant_probe(path: &RelativePath) -> String {
    // Directory metrics are evaluated at the directory path, but users usually
    // write recursive path overrides such as `src/**`. Matching a synthetic
    // descendant lets that override cover the `src` directory metric too.
    if path.as_str().is_empty() {
        return DIRECTORY_OVERRIDE_DESCENDANT_PROBE.to_owned();
    }
    format!("{}/{DIRECTORY_OVERRIDE_DESCENDANT_PROBE}", path.as_str())
}

fn source_parent_directory(path: &RelativePath) -> Result<RelativePath> {
    let path = path.to_path_buf();
    RelativePath::new(path.parent().unwrap_or_else(|| Path::new("")))
}
