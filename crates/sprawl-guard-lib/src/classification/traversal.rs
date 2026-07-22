use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use ignore::DirEntry;
use ignore::WalkBuilder;

use crate::config::Config;
use crate::value::path::normalize_under_root;
use crate::{DiagnosticDetail, RelativePath, Result, SprawlError};

use super::types::{TraversalIncident, TraversalIncidentKind, TraversalIncidentSeverity};

/// Creates an ignore-aware repository walk from the resolved ignore config.
pub(super) fn walk_sources(root: &Path, config: &Config) -> ignore::Walk {
    walk_sources_scoped(root, config, &[])
}

/// Creates an ignore-aware walk pruned to repository-relative scopes.
pub(super) fn walk_sources_scoped(
    root: &Path,
    config: &Config,
    scopes: &[RelativePath],
) -> ignore::Walk {
    let scopes = scopes
        .iter()
        .map(|scope| scope.as_str().to_owned())
        .collect::<Vec<_>>();
    let mut builder = walk_builder(root, config);
    if !scopes.is_empty() {
        let root = root.to_path_buf();
        builder.filter_entry(move |entry| {
            let Ok(path) = normalize_under_root(&root, entry.path()) else {
                return false;
            };
            is_scope_relevant(path.as_str(), &scopes)
        });
    }
    builder.build()
}

fn walk_builder(root: &Path, config: &Config) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(config.ignore.skip_hidden)
        .ignore(config.ignore.respect_ignore)
        .git_ignore(config.ignore.respect_gitignore)
        .git_global(config.ignore.respect_gitignore)
        .git_exclude(config.ignore.respect_gitignore)
        .follow_links(config.traversal.follow_symlinks.follows_directories())
        .max_depth(Some(config.traversal.max_depth.get()));
    #[cfg(not(target_os = "wasi"))]
    builder.same_file_system(config.traversal.same_file_system);
    // `walkdir` rejects filesystem-boundary checks on WASI. Preopens limit the
    // exposed host paths, but not mount traversal within a preopened tree.
    builder.require_git(false);
    if config.ignore.respect_rgignore {
        builder.add_custom_ignore_filename(".rgignore");
    }
    if config.ignore.respect_fdignore {
        builder.add_custom_ignore_filename(".fdignore");
    }
    builder
}

/// Returns whether an existing file appears in the configured ignore-aware walk.
///
/// `explain` uses this to distinguish files ignored by traversal from files that
/// are visible to traversal but later excluded by sprawl-guard policy.
pub(super) fn is_walked_file(root: &Path, path: &Path, config: &Config) -> Result<bool> {
    let expected = normalize_under_root(root, path)?;
    let mut visited_entries = 0_u32;
    for result in walk_sources(root, config) {
        let entry = checked_walk_entry(root, config, &mut visited_entries, result)?;
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !is_source_file_candidate(entry.path(), file_type, config)? {
            continue;
        }
        if normalize_under_root(root, entry.path())? == expected {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Converts one walker result while enforcing the configured traversal entry limit.
pub(super) fn checked_walk_entry(
    root: &Path,
    config: &Config,
    visited_entries: &mut u32,
    result: std::result::Result<DirEntry, ignore::Error>,
) -> Result<DirEntry> {
    let entry = result.map_err(|source| SprawlError::Traverse {
        path: root.to_path_buf(),
        source,
    })?;
    *visited_entries =
        visited_entries
            .checked_add(1)
            .ok_or_else(|| SprawlError::TraversalEntryLimitExceeded {
                root: root.to_path_buf(),
                limit: config.traversal.max_entries.get(),
            })?;
    if *visited_entries > config.traversal.max_entries.get() {
        return Err(SprawlError::TraversalEntryLimitExceeded {
            root: root.to_path_buf(),
            limit: config.traversal.max_entries.get(),
        });
    }
    Ok(entry)
}

/// Converts one walker result for discovery while collecting recoverable incidents.
///
/// The ignore walker can continue after many descendant errors. Discovery uses
/// this path so `check` can report every recoverable traversal incident instead
/// of failing on the first one.
pub(super) fn collect_walk_entry(
    root: &Path,
    config: &Config,
    severity: TraversalIncidentSeverity,
    visited_entries: &mut u32,
    incidents: &mut Vec<TraversalIncident>,
    result: std::result::Result<DirEntry, ignore::Error>,
) -> Result<Option<DirEntry>> {
    let entry = match result {
        Ok(entry) => entry,
        Err(source) => {
            incidents.extend(traversal_incidents_from_ignore_error(
                root, severity, source,
            ));
            return Ok(None);
        }
    };
    *visited_entries = visited_entries
        .checked_add(1)
        .ok_or_else(|| traversal_limit_error(root, config, severity))?;
    if *visited_entries > config.traversal.max_entries.get() {
        return Err(traversal_limit_error(root, config, severity));
    }
    Ok(Some(entry))
}

/// Converts a file-type read failure before classification into a traversal incident.
pub(super) fn file_type_incident(
    root: &Path,
    severity: TraversalIncidentSeverity,
    path: &Path,
    detail: DiagnosticDetail,
) -> TraversalIncident {
    TraversalIncident {
        kind: TraversalIncidentKind::FileTypeFailed,
        severity,
        path: normalize_under_root(root, path).ok(),
        detail,
    }
}

fn traversal_limit_error(
    root: &Path,
    config: &Config,
    severity: TraversalIncidentSeverity,
) -> SprawlError {
    SprawlError::TraversalFailed {
        root: root.to_path_buf(),
        incidents: vec![TraversalIncident {
            kind: TraversalIncidentKind::EntryLimitExceeded,
            severity,
            path: None,
            detail: DiagnosticDetail::new(format!(
                "saw more than {} filesystem entries",
                config.traversal.max_entries.get()
            )),
        }],
    }
}

fn traversal_incidents_from_ignore_error(
    root: &Path,
    severity: TraversalIncidentSeverity,
    source: ignore::Error,
) -> Vec<TraversalIncident> {
    let mut incidents = Vec::new();
    collect_ignore_error(root, severity, source, None, None, &mut incidents);
    incidents
}

fn collect_ignore_error(
    root: &Path,
    severity: TraversalIncidentSeverity,
    source: ignore::Error,
    path: Option<&Path>,
    depth: Option<usize>,
    incidents: &mut Vec<TraversalIncident>,
) {
    match source {
        ignore::Error::Partial(errors) => {
            for error in errors {
                collect_ignore_error(root, severity, error, path, depth, incidents);
            }
        }
        ignore::Error::WithLineNumber { err, .. } => {
            collect_ignore_error(root, severity, *err, path, depth, incidents);
        }
        ignore::Error::WithPath { path, err } => {
            collect_ignore_error(root, severity, *err, Some(&path), depth, incidents);
        }
        ignore::Error::WithDepth { depth, err } => {
            collect_ignore_error(root, severity, *err, path, Some(depth), incidents);
        }
        ignore::Error::Loop { child, .. } => {
            incidents.push(TraversalIncident {
                kind: TraversalIncidentKind::SymlinkLoop,
                severity,
                path: normalize_under_root(root, &child).ok(),
                detail: DiagnosticDetail::new("symlink loop found"),
            });
        }
        ignore::Error::Io(error) => {
            let kind = if error.kind() == ErrorKind::NotFound {
                TraversalIncidentKind::BrokenSymlink
            } else if depth.is_some() {
                TraversalIncidentKind::ReadDirFailed
            } else {
                TraversalIncidentKind::FileTypeFailed
            };
            incidents.push(TraversalIncident {
                kind,
                severity,
                path: path.and_then(|path| normalize_under_root(root, path).ok()),
                detail: DiagnosticDetail::new(error.to_string()),
            });
        }
        ignore::Error::Glob { err, .. } | ignore::Error::UnrecognizedFileType(err) => {
            incidents.push(TraversalIncident {
                kind: TraversalIncidentKind::IgnoreParseFailed,
                severity,
                path: path.and_then(|path| normalize_under_root(root, path).ok()),
                detail: DiagnosticDetail::new(err),
            });
        }
        ignore::Error::InvalidDefinition => {
            incidents.push(TraversalIncident {
                kind: TraversalIncidentKind::IgnoreParseFailed,
                severity,
                path: path.and_then(|path| normalize_under_root(root, path).ok()),
                detail: DiagnosticDetail::new("invalid file type definition"),
            });
        }
    }
}

/// Returns whether a walked entry can be treated as a source file candidate.
///
/// Normal files always qualify. Symlinked files qualify only when traversal
/// config opts into file symlinks. Symlinked directories are handled by the
/// walker, not here, because they affect traversal topology.
pub(super) fn is_source_file_candidate(
    path: &Path,
    file_type: fs::FileType,
    config: &Config,
) -> Result<bool> {
    if file_type.is_file() {
        if path_is_symlink(path)? {
            return Ok(config.traversal.follow_symlinks.follows_files());
        }
        return Ok(true);
    }
    if !file_type.is_symlink() || !config.traversal.follow_symlinks.follows_files() {
        return Ok(false);
    }
    target_is_regular_file(path)
}

fn path_is_symlink(path: &Path) -> Result<bool> {
    let metadata = fs::symlink_metadata(path).map_err(|source| SprawlError::ReadFileType {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(metadata.file_type().is_symlink())
}

fn target_is_regular_file(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_file()),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(false),
        Err(source) => Err(SprawlError::ReadFileType {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn is_scope_relevant(path: &str, scopes: &[String]) -> bool {
    path.is_empty()
        || scopes
            .iter()
            .any(|scope| path_is_at_or_under(path, scope) || path_is_at_or_under(scope, path))
}

fn path_is_at_or_under(path: &str, prefix: &str) -> bool {
    prefix.is_empty()
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}
