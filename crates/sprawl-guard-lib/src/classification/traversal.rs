use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use ignore::DirEntry;
use ignore::WalkBuilder;

use crate::config::Config;
use crate::value::path::normalize_under_root;
use crate::{DiagnosticDetail, RelativePath, Result, SprawlError};

use super::types::{UnreadablePath, UnreadablePathKind, UnreadablePathSeverity};

/// Creates an ignore-aware walk pruned to repository-relative selected paths.
pub(super) fn walk_sources_scoped(
    root: &Path,
    config: &Config,
    scopes: &[RelativePath],
) -> impl Iterator<Item = std::result::Result<DirEntry, ignore::Error>> {
    let scopes = scopes
        .iter()
        .map(|scope| root.join(scope.to_path_buf()))
        .collect::<Vec<_>>();
    let mut builder = walk_builder(root, config);
    let filter_root = root.to_path_buf();
    let skip_hidden = config.ignore.skip_hidden;
    builder.filter_entry(move |entry| {
        let hidden = entry.path() != filter_root
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with('.'));
        (!skip_hidden || !hidden)
            && (scopes.is_empty() || is_native_scope_relevant(entry.path(), &filter_root, &scopes))
    });
    // ignore 0.4 still parses parent ignore files with parents(false). Their
    // errors are outside this scan's configuration and observation limit.
    let root = root.to_path_buf();
    builder.build().filter(move |result| match result {
        Ok(_) => true,
        Err(error) => !is_parent_ignore_error(&root, error),
    })
}

fn is_parent_ignore_error(root: &Path, error: &ignore::Error) -> bool {
    match error {
        ignore::Error::Partial(errors) => errors
            .iter()
            .all(|error| is_parent_ignore_error(root, error)),
        ignore::Error::WithPath { path, .. } => {
            !path.starts_with(root) && path.parent().is_some_and(|parent| root.starts_with(parent))
        }
        ignore::Error::WithLineNumber { err, .. } | ignore::Error::WithDepth { err, .. } => {
            is_parent_ignore_error(root, err)
        }
        ignore::Error::Loop { .. }
        | ignore::Error::Io(_)
        | ignore::Error::Glob { .. }
        | ignore::Error::UnrecognizedFileType(_)
        | ignore::Error::InvalidDefinition => false,
    }
}

fn walk_builder(root: &Path, config: &Config) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        // Git stores names, not Windows hidden attributes.
        .hidden(false)
        .ignore(config.ignore.respect_ignore)
        .git_ignore(config.ignore.respect_gitignore)
        // Only ignore files inside the root affect the scan.
        .parents(false)
        .git_global(false)
        .git_exclude(false)
        .follow_links(config.traversal.follow_symlinks.follows_directories());
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
/// are visible to the walk but later excluded by the configuration.
pub(super) fn is_walked_file(root: &Path, path: &Path, config: &Config) -> Result<bool> {
    let expected = normalize_under_root(root, path)?;
    let mut visited_entries = 0_u32;
    for result in walk_sources_scoped(root, config, std::slice::from_ref(&expected)) {
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
    reject_symlinked_ignore_files(&entry, config)?;
    count_walk_entry(root, config, visited_entries)?;
    if let Some(source) = entry.error() {
        return Err(SprawlError::Traverse {
            path: entry.path().to_path_buf(),
            source: source.clone(),
        });
    }
    Ok(entry)
}

/// Converts one walker result for discovery while collecting recoverable unreadable paths.
///
/// The ignore walker can continue after many descendant errors. Discovery uses
/// this path so `check` can report every recoverable unreadable path instead
/// of failing on the first one. The hard entry bound is always an operational
/// error and is not affected by `unreadable_paths`.
pub(super) fn collect_walk_entry(
    root: &Path,
    config: &Config,
    severity: UnreadablePathSeverity,
    visited_entries: &mut u32,
    unreadable_paths: &mut Vec<UnreadablePath>,
    result: std::result::Result<DirEntry, ignore::Error>,
) -> Result<Option<DirEntry>> {
    count_walk_entry(root, config, visited_entries)?;
    let entry = match result {
        Ok(entry) => entry,
        Err(source) => {
            unreadable_paths.extend(unreadable_paths_from_ignore_error(root, severity, source));
            return Ok(None);
        }
    };
    reject_symlinked_ignore_files(&entry, config)?;
    if let Some(source) = entry.error() {
        unreadable_paths.extend(unreadable_paths_from_ignore_error(
            root,
            severity,
            source.clone(),
        ));
    }
    Ok(Some(entry))
}

/// Refuses ignore files whose contents are not owned by the scanned directory.
fn reject_symlinked_ignore_files(entry: &DirEntry, config: &Config) -> Result<()> {
    if !entry.file_type().is_some_and(|kind| kind.is_dir()) {
        return Ok(());
    }
    for (name, enabled) in [
        (".ignore", config.ignore.respect_ignore),
        (".gitignore", config.ignore.respect_gitignore),
        (".rgignore", config.ignore.respect_rgignore),
        (".fdignore", config.ignore.respect_fdignore),
    ] {
        let path = entry.path().join(name);
        if enabled && fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err(SprawlError::Traverse {
                path,
                source: ignore::Error::Io(std::io::Error::other(
                    "ignore file must not be a symlink",
                )),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::test_support::{load_config, write_rust_entry_limit_config};

    mod when_the_scan_root_has_disappeared {
        use super::*;

        #[test]
        fn it_preserves_the_root_filesystem_error() {
            let directory = tempfile::tempdir().unwrap();
            let config = load_config(directory.path());
            let root = directory.path().join("missing");

            let results = walk_sources_scoped(&root, &config, &[]).collect::<Vec<_>>();

            assert_eq!(results.len(), 1);
            assert!(results[0].is_err());
        }
    }

    mod when_walk_errors_exhaust_the_entry_budget {
        use super::*;

        #[test]
        fn it_counts_each_error_before_collecting_the_next_unreadable_path() {
            let root = tempfile::tempdir().unwrap();
            write_rust_entry_limit_config(root.path(), 1);
            let config = load_config(root.path());
            let mut visited_entries = 0;
            let mut unreadable_paths = Vec::new();

            let first = collect_walk_entry(
                root.path(),
                &config,
                UnreadablePathSeverity::Warning,
                &mut visited_entries,
                &mut unreadable_paths,
                Err(ignore::Error::Io(io::Error::other("first failure"))),
            )
            .unwrap();
            let second = collect_walk_entry(
                root.path(),
                &config,
                UnreadablePathSeverity::Warning,
                &mut visited_entries,
                &mut unreadable_paths,
                Err(ignore::Error::Io(io::Error::other("second failure"))),
            )
            .unwrap_err();

            assert!(first.is_none());
            assert_eq!(unreadable_paths.len(), 1);
            assert!(matches!(
                second,
                SprawlError::TraversalEntryLimitExceeded { limit: 1, .. }
            ));
        }
    }
}

fn count_walk_entry(root: &Path, config: &Config, visited_entries: &mut u32) -> Result<()> {
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
    Ok(())
}

/// Converts a file-type read failure before classification into a unreadable path.
pub(super) fn file_type_unreadable_path(
    root: &Path,
    severity: UnreadablePathSeverity,
    path: &Path,
    detail: DiagnosticDetail,
) -> UnreadablePath {
    UnreadablePath {
        kind: UnreadablePathKind::FileTypeFailed,
        severity,
        path: normalize_under_root(root, path).ok(),
        detail,
    }
}

fn unreadable_paths_from_ignore_error(
    root: &Path,
    severity: UnreadablePathSeverity,
    source: ignore::Error,
) -> Vec<UnreadablePath> {
    let mut unreadable_paths = Vec::new();
    collect_ignore_error(root, severity, source, None, None, &mut unreadable_paths);
    unreadable_paths
}

fn collect_ignore_error(
    root: &Path,
    severity: UnreadablePathSeverity,
    source: ignore::Error,
    path: Option<&Path>,
    depth: Option<usize>,
    unreadable_paths: &mut Vec<UnreadablePath>,
) {
    match source {
        ignore::Error::Partial(errors) => {
            for error in errors {
                collect_ignore_error(root, severity, error, path, depth, unreadable_paths);
            }
        }
        ignore::Error::WithLineNumber { err, .. } => {
            collect_ignore_error(root, severity, *err, path, depth, unreadable_paths);
        }
        ignore::Error::WithPath { path, err } => {
            collect_ignore_error(root, severity, *err, Some(&path), depth, unreadable_paths);
        }
        ignore::Error::WithDepth { depth, err } => {
            collect_ignore_error(root, severity, *err, path, Some(depth), unreadable_paths);
        }
        ignore::Error::Loop { child, .. } => {
            unreadable_paths.push(UnreadablePath {
                kind: UnreadablePathKind::SymlinkLoop,
                severity,
                path: normalize_under_root(root, &child).ok(),
                detail: DiagnosticDetail::new("symlink loop found"),
            });
        }
        ignore::Error::Io(error) => {
            let kind = if error.kind() == ErrorKind::NotFound {
                UnreadablePathKind::BrokenSymlink
            } else if depth.is_some() {
                UnreadablePathKind::ReadDirFailed
            } else {
                UnreadablePathKind::FileTypeFailed
            };
            unreadable_paths.push(UnreadablePath {
                kind,
                severity,
                path: path.and_then(|path| normalize_under_root(root, path).ok()),
                detail: DiagnosticDetail::new(error.to_string()),
            });
        }
        ignore::Error::Glob { err, .. } | ignore::Error::UnrecognizedFileType(err) => {
            unreadable_paths.push(UnreadablePath {
                kind: UnreadablePathKind::IgnoreParseFailed,
                severity,
                path: path.and_then(|path| normalize_under_root(root, path).ok()),
                detail: DiagnosticDetail::new(err),
            });
        }
        ignore::Error::InvalidDefinition => {
            unreadable_paths.push(UnreadablePath {
                kind: UnreadablePathKind::IgnoreParseFailed,
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

fn is_native_scope_relevant(path: &Path, root: &Path, scopes: &[PathBuf]) -> bool {
    path == root
        || scopes
            .iter()
            .any(|scope| path.starts_with(scope) || scope.starts_with(path))
}
