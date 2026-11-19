//! Source discovery and path classification.

use std::path::Path;

use crate::config::{Config, UnreadablePathPolicy};
use crate::resolved_limits::LimitResolver;
use crate::value::path::{normalize_under_root, resolve_root};
use crate::{DiagnosticDetail, RelativePath, Result, SprawlError};

use super::classifier::Classifier;
use super::traversal::{
    collect_walk_entry, file_type_unreadable_path, is_source_file_candidate, walk_sources_scoped,
};
use super::types::{
    ByteLimitedFile, ClassifiedSource, PathClassification, SourceDiscoveryReport,
    UnreadablePathSeverity,
};

/// Discovers included source files under the repository root.
///
/// Discovery applies the configured ignore files first, then applies global file
/// exclusions, per-language include/exclude patterns, and test globs. Ignored
/// paths, non-source files, globally excluded files, language-excluded files,
/// and disabled test files are omitted from the result. Ambiguous language matches are returned as errors
/// because checks need one definitive set of language settings for each source
/// file. Paths returned from this function are repository-relative and sorted
/// deterministically for stable reports. Because this simple API cannot return
/// unreadable paths, any unreadable path below the root is reported as an
/// error, whatever `traversal.unreadable_paths` says.
pub fn discover_sources(root: &Path, config: &Config) -> Result<Vec<ClassifiedSource>> {
    let report =
        discover_sources_report(root, config, &[], Some(UnreadablePathPolicy::Fail), None)?;
    if !report.unreadable_paths.is_empty() {
        return Err(SprawlError::TraversalFailed {
            root: resolve_root(root)?,
            unreadable_paths: report.unreadable_paths,
        });
    }
    Ok(report.sources)
}

/// Discovers source files, and the paths below the root that could not be read.
///
/// The walk skips paths outside `scopes` when any are given, so a check of selected paths does
/// not visit unrelated subtrees. The unreadable-path policy sets only each path's severity;
/// discovery always finishes. With `byte_limits`, discovery also lists every file whose resolved
/// byte limit is finite; without, that list is empty.
pub(crate) fn discover_sources_report(
    root: &Path,
    config: &Config,
    scopes: &[RelativePath],
    unreadable_paths_override: Option<UnreadablePathPolicy>,
    byte_limits: Option<&LimitResolver<'_>>,
) -> Result<SourceDiscoveryReport> {
    let root = resolve_root(root)?;
    let classifier = Classifier::new(config)?;
    let mut sources = Vec::new();
    let mut unreadable_paths = Vec::new();
    let mut byte_limited_files = Vec::new();
    let byte_limits = byte_limits.filter(|limits| limits.limits_file_bytes());
    let mut visited_entries = 0_u32;
    let policy = unreadable_paths_override.unwrap_or(config.traversal.unreadable_paths);
    let severity = UnreadablePathSeverity::from(policy);

    for result in walk_sources_scoped(&root, config, scopes) {
        let Some(entry) = collect_walk_entry(
            &root,
            config,
            severity,
            &mut visited_entries,
            &mut unreadable_paths,
            result,
        )?
        else {
            continue;
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        let is_source_candidate = match is_source_file_candidate(entry.path(), file_type, config) {
            Ok(value) => value,
            Err(SprawlError::ReadFileType { path, source }) => {
                unreadable_paths.push(file_type_unreadable_path(
                    &root,
                    severity,
                    &path,
                    DiagnosticDetail::new(source.to_string()),
                ));
                continue;
            }
            Err(source) => return Err(source),
        };
        if !is_source_candidate {
            continue;
        }
        let classification = classifier.classify(normalize_under_root(&root, entry.path())?)?;
        if let Some(limits) = byte_limits {
            byte_limited_files.extend(byte_limited_file(limits, &classification));
        }
        if let Some(source) = Classifier::included_source(classification)? {
            sources.push(source);
        }
    }

    sources.sort_by(|left, right| left.path.cmp(&right.path));
    byte_limited_files.sort_by(|left, right| left.path.cmp(&right.path));
    unreadable_paths.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    Ok(SourceDiscoveryReport {
        sources,
        unreadable_paths,
        byte_limited_files,
    })
}

/// Returns the file a classification names, when byte limits apply to it and its limit is finite.
fn byte_limited_file(
    limits: &LimitResolver<'_>,
    classification: &PathClassification<ClassifiedSource>,
) -> Option<ByteLimitedFile> {
    let path = classification.byte_limited_path(|source| &source.path)?;
    let limit = limits.max_bytes_per_file(path).finite()?;
    Some(ByteLimitedFile {
        path: path.clone(),
        limit,
    })
}

#[cfg(test)]
mod matcher_tests;

#[cfg(test)]
#[path = "non_utf8_discovery_tests.rs"]
mod non_utf8_tests;

#[cfg(test)]
mod tests;
