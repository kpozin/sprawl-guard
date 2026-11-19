//! Resolved limits after applying language and path overrides.

use std::num::NonZeroUsize;
use std::path::Path;

use globset::GlobMatcher;
use serde::Serialize;

use crate::classification::{ClassifiedSource, SourceKind};
use crate::config::{Config, OverrideLimits, OverridePaths, OverrideReason};
use crate::{
    CountLines, DirectFileLimit, FileStorage, LanguageGroupId, LineLimit, MaxBytesPerFile,
    RelativePath, Result,
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

/// Resolved line-limit configuration for one file metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedFileLinesLimit {
    /// Line types counted in the measurement.
    pub count_lines: CountLines,
    /// Final line limit after precedence is applied.
    pub value: LineLimit,
    /// Overrides whose path globs matched the file path.
    pub matched_overrides: Vec<MatchedOverride>,
}

/// Resolved direct-file limit for one directory metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedDirectFileLimit {
    /// Final direct-file limit after precedence is applied.
    pub value: DirectFileLimit,
    /// Overrides whose path globs matched the directory metric path.
    ///
    /// A trailing `/**` pattern also matches the prefix directory itself for
    /// directory metrics. For example, `src/**` matches both `src` and
    /// `src/generated`.
    pub matched_overrides: Vec<MatchedOverride>,
}

/// Resolved byte limit for one file, source or not.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedFileBytesLimit {
    /// Final byte limit after precedence is applied.
    pub value: MaxBytesPerFile,
    /// Overrides whose path globs matched the file path.
    pub matched_overrides: Vec<MatchedOverride>,
}

/// Resolved file and directory limits for an included source path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceResolvedLimits {
    /// Resolved file line limit for the source's production/test kind.
    pub file: ResolvedFileLinesLimit,
    /// Resolved directory direct-file limit for the source's parent directory.
    pub directory: ResolvedDirectFileLimit,
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
                        paths: override_path_matchers(&override_entry.paths),
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
            file: self.file_lines_limit(source),
            directory: self.directory_direct_files(
                source.kind,
                source.group(),
                &source_parent_directory(&source.path)?,
            ),
        })
    }

    /// Resolves the file line limit for one included source.
    pub(crate) fn file_lines_limit(&self, source: &ClassifiedSource) -> ResolvedFileLinesLimit {
        let language_limits = self
            .config
            .effective_language_file_limits(source.language());
        let count_lines = language_limits.count_lines;
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

        ResolvedFileLinesLimit {
            count_lines,
            value: limit,
            matched_overrides,
        }
    }

    /// Returns whether any file can have a finite byte limit, so that file sizes need measuring.
    pub(crate) fn limits_file_bytes(&self) -> bool {
        [FileStorage::Regular, FileStorage::Lfs]
            .into_iter()
            .any(|storage| {
                self.config
                    .limits
                    .max_file_bytes(storage)
                    .finite()
                    .is_some()
                    || self.overrides.iter().any(|override_entry| {
                        override_entry
                            .limits
                            .max_file_bytes(storage)
                            .and_then(MaxBytesPerFile::finite)
                            .is_some()
                    })
            })
    }

    /// Resolves the byte limit for one file stored in `storage`: the repository default, replaced
    /// by each matching override that sets one, in declaration order.
    pub(crate) fn max_file_bytes(
        &self,
        path: &RelativePath,
        storage: FileStorage,
    ) -> MaxBytesPerFile {
        self.byte_limit_overrides(path, storage)
            .map(|(_, limit)| limit)
            .last()
            .unwrap_or(self.config.limits.max_file_bytes(storage))
    }

    /// Resolves the byte limit for one file stored in `storage`, with the overrides that set it.
    pub(crate) fn file_bytes_limit(
        &self,
        path: &RelativePath,
        storage: FileStorage,
    ) -> ResolvedFileBytesLimit {
        ResolvedFileBytesLimit {
            value: self.max_file_bytes(path, storage),
            matched_overrides: self
                .byte_limit_overrides(path, storage)
                .map(|(override_entry, _)| override_entry.matched_override())
                .collect(),
        }
    }

    /// Returns the overrides that match a file and set its byte limit for `storage`, in
    /// declaration order.
    fn byte_limit_overrides<'resolver>(
        &'resolver self,
        path: &'resolver RelativePath,
        storage: FileStorage,
    ) -> impl Iterator<Item = (&'resolver CompiledOverride<'config>, MaxBytesPerFile)> {
        self.overrides.iter().filter_map(move |override_entry| {
            let limit = override_entry.limits.max_file_bytes(storage)?;
            file_matches_override(&override_entry.paths, path).then_some((override_entry, limit))
        })
    }

    /// Resolves the direct-file limit for one directory metric.
    pub(crate) fn directory_direct_files(
        &self,
        kind: SourceKind,
        group: &LanguageGroupId,
        path: &RelativePath,
    ) -> ResolvedDirectFileLimit {
        let group_limits = self.config.effective_language_group_directory_limits(group);
        let mut limit = match kind {
            SourceKind::Production => group_limits.production,
            SourceKind::Test => group_limits.test,
        };
        let mut matched_overrides = Vec::new();

        for override_entry in &self.overrides {
            if !directory_matches_override(&override_entry.paths, path) {
                continue;
            }
            matched_overrides.push(override_entry.matched_override());
            match kind {
                SourceKind::Production => {
                    if let Some(value) = override_entry
                        .limits
                        .production_direct_files_per_directory()
                    {
                        limit = value;
                    }
                }
                SourceKind::Test => {
                    if let Some(value) = override_entry.limits.test_direct_files_per_directory() {
                        limit = value;
                    }
                }
            }
        }

        ResolvedDirectFileLimit {
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

fn override_path_matchers(paths: &OverridePaths) -> Vec<OverridePathMatcher> {
    paths
        .as_slice()
        .iter()
        .map(|path| OverridePathMatcher {
            matches_directory_descendants: path.as_str().ends_with("/**"),
            matcher: path.compile().compile_matcher(),
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
