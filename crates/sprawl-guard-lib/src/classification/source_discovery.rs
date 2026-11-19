//! Source discovery and path classification.

use std::collections::BTreeMap;
use std::path::Path;

use crate::config::{Config, UnreadablePathPolicy};
use crate::resolved_limits::LimitResolver;
use crate::value::path::{normalize_under_root, resolve_root};
use crate::{DiagnosticDetail, FileStorage, RelativePath, Result, SprawlError};

use super::classifier::Classifier;
use super::traversal::{
    collect_walk_entry, file_type_unreadable_path, is_source_file_candidate, walk_sources_scoped,
};
use super::types::{
    ByteLimitedFile, ClassifiedSource, PathClassification, SourceDiscoveryReport, UnreadablePath,
    UnreadablePathKind, UnreadablePathSeverity,
};
use crate::source_file::LfsAttributes;

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
    let mut unknown_storage_files = Vec::new();
    let byte_limits = byte_limits.filter(|limits| limits.limits_file_bytes());
    let mut lfs_attributes = LfsAttributes::new(&root);
    let mut visited_entries = 0_u32;
    let policy = unreadable_paths_override.unwrap_or(config.traversal.unreadable_paths);
    let severity = UnreadablePathSeverity::from(policy);

    let mut unreached_scopes = scopes
        .iter()
        .map(|scope| (root.join(scope.to_path_buf()), scope))
        .collect::<BTreeMap<_, _>>();

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
        unreached_scopes.remove(entry.path());
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
            match byte_limit_lookup(limits, &mut lfs_attributes, &classification) {
                Some(ByteLimitLookup::Limited(file)) => byte_limited_files.push(file),
                Some(ByteLimitLookup::UnknownStorage(path)) => unknown_storage_files.push(path),
                None => {}
            }
        }
        if let Some(source) = Classifier::included_source(classification)? {
            sources.push(source);
        }
    }

    // An unreadable `.gitattributes` file leaves the storage of every file in its directory and
    // below unknown, so the gap covers that directory. The root, and any directory above it, has
    // no non-empty relative path, so its gap covers the whole tree.
    unreadable_paths.extend(lfs_attributes.take_failures().into_iter().map(|failure| {
        UnreadablePath {
            kind: UnreadablePathKind::ReadGitAttributesFailed,
            severity,
            path: failure
                .path
                .parent()
                .and_then(|directory| normalize_under_root(&root, directory).ok()),
            detail: DiagnosticDetail::new(failure.source.to_string()),
        }
    }));
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    byte_limited_files.sort_by(|left, right| left.path.cmp(&right.path));
    unreadable_paths.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    Ok(SourceDiscoveryReport {
        sources,
        unreadable_paths,
        byte_limited_files,
        unknown_storage_files,
        skipped_scopes: unreached_scopes.into_values().cloned().collect(),
    })
}

/// What discovery learns about the byte limit of a file that has a finite limit for some storage.
enum ByteLimitLookup {
    /// The file's storage is known and its limit for that storage is finite.
    Limited(ByteLimitedFile),
    /// A `.gitattributes` file above the file can't be read, so either limit could apply.
    UnknownStorage(RelativePath),
}

/// Looks up the byte limit of the file a classification names, when byte limits apply to it.
///
/// A file with no finite limit for either storage is skipped before `.gitattributes` is read, so an
/// unreadable `.gitattributes` affects only the files whose limit depends on it. A file whose
/// storage is known but whose limit for that storage is unlimited is skipped too.
fn byte_limit_lookup(
    limits: &LimitResolver<'_>,
    lfs_attributes: &mut LfsAttributes,
    classification: &PathClassification<ClassifiedSource>,
) -> Option<ByteLimitLookup> {
    let path = classification.byte_limited_path(|source| &source.path)?;
    if FileStorage::ALL
        .into_iter()
        .all(|storage| limits.max_file_bytes(path, storage).finite().is_none())
    {
        return None;
    }
    let Some(storage) = lfs_attributes.storage(path) else {
        return Some(ByteLimitLookup::UnknownStorage(path.clone()));
    };
    let limit = limits.max_file_bytes(path, storage).finite()?;
    Some(ByteLimitLookup::Limited(ByteLimitedFile {
        path: path.clone(),
        storage,
        limit,
    }))
}

#[cfg(test)]
mod matcher_tests;

#[cfg(test)]
#[path = "non_utf8_discovery_tests.rs"]
mod non_utf8_tests;

#[cfg(test)]
mod tests;
