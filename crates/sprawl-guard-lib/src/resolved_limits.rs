//! Resolved policy limits after applying language and path overrides.

use std::num::NonZeroUsize;
use std::path::Path;

use globset::{Glob, GlobMatcher};
use serde::Serialize;

use crate::classification::{ClassifiedSource, SourceKind};
use crate::config::{Config, OverrideLimits, OverridePaths, OverrideReason};
use crate::{
    IncludedLineTypes, LanguageGroupId, LeafFileLimit, LocLimit, RelativePath, Result, SprawlError,
};

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
    /// Rationale from the matching override.
    pub reason: OverrideReason,
}

/// Resolved configured-LOC policy for one file metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedFileLocLimit {
    /// Logical line categories included in the measurement.
    pub included_line_types: IncludedLineTypes,
    /// Final configured-LOC limit after precedence is applied.
    pub value: LocLimit,
    /// Overrides whose path globs matched the file path.
    pub matched_overrides: Vec<MatchedOverride>,
}

/// Resolved leaf-file limit for one directory metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedLeafFileLimit {
    /// Final direct leaf-file limit after precedence is applied.
    pub value: LeafFileLimit,
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
    pub file: ResolvedFileLocLimit,
    /// Resolved directory fan-out limit for the source's parent directory.
    pub directory: ResolvedLeafFileLimit,
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
                        reason: &override_entry.reason,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }

    /// Resolves both file and parent-directory limits for one included source.
    pub(crate) fn source_limits(&self, source: &ClassifiedSource) -> Result<SourceResolvedLimits> {
        Ok(SourceResolvedLimits {
            file: self.file_loc_limit(source),
            directory: self.directory_leaf_files(
                source.kind,
                source.group(),
                &source_parent_directory(&source.path)?,
            ),
        })
    }

    /// Resolves the file LOC limit for one included source.
    pub(crate) fn file_loc_limit(&self, source: &ClassifiedSource) -> ResolvedFileLocLimit {
        let language_limits = self
            .config
            .effective_language_file_limits(source.language());
        let included_line_types = language_limits.included_line_types;
        let mut limit = match source.kind {
            SourceKind::Production => language_limits.production,
            SourceKind::Test => language_limits.test,
        };
        let mut matched_overrides = Vec::new();

        for override_entry in &self.overrides {
            if !file_matches_override(&override_entry.paths, &source.path) {
                continue;
            }
            matched_overrides.push(override_entry.matched_override());
            match source.kind {
                SourceKind::Production => {
                    if let Some(value) = override_entry.limits.production_loc_per_file() {
                        limit = value;
                    }
                }
                SourceKind::Test => {
                    if let Some(value) = override_entry.limits.test_loc_per_file() {
                        limit = value;
                    }
                }
            }
        }

        ResolvedFileLocLimit {
            included_line_types,
            value: limit,
            matched_overrides,
        }
    }

    /// Resolves the direct leaf-file limit for one directory metric.
    pub(crate) fn directory_leaf_files(
        &self,
        kind: SourceKind,
        group: &LanguageGroupId,
        path: &RelativePath,
    ) -> ResolvedLeafFileLimit {
        let mut limit = match group {
            LanguageGroupId::Documentation => LeafFileLimit::Unlimited,
            LanguageGroupId::Source | LanguageGroupId::Custom(_) => match kind {
                SourceKind::Production => {
                    self.config.limits.max_leaf_files_per_directory.production
                }
                SourceKind::Test => self.config.limits.max_leaf_files_per_directory.test,
            },
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
                        limit = value;
                    }
                }
                SourceKind::Test => {
                    if let Some(value) = override_entry.limits.test_leaf_files_per_directory() {
                        limit = value;
                    }
                }
            }
        }

        ResolvedLeafFileLimit {
            value: limit,
            matched_overrides,
        }
    }
}

struct CompiledOverride<'config> {
    ordinal: OverrideOrdinal,
    paths: Vec<OverridePathMatcher>,
    limits: &'config OverrideLimits,
    original_paths: &'config OverridePaths,
    reason: &'config OverrideReason,
}

impl CompiledOverride<'_> {
    fn matched_override(&self) -> MatchedOverride {
        MatchedOverride {
            ordinal: self.ordinal,
            paths: self.original_paths.clone(),
            limits: self.limits.clone(),
            reason: self.reason.clone(),
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
