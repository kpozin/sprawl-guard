use std::path::{Path, PathBuf};

use sprawl_guard_lib::{RatchetFile, RelativePath};

use crate::error::Result;

pub(super) enum RatchetScope {
    All,
    Paths(Vec<RelativePath>),
}

impl RatchetScope {
    pub(super) fn contains(&self, path: &RelativePath) -> bool {
        match self {
            Self::All => true,
            Self::Paths(paths) => paths.iter().any(|scope| path.is_at_or_under(scope)),
        }
    }
}

/// Splits a `tighten PATHS...` request into the entries it may edit and the paths it measures.
///
/// The returned `RatchetScope` controls which existing ratchet entries may change or be removed.
/// The returned paths contain only selected paths that still exist on disk and are measured by
/// `check_without_ratchet`. Missing relative paths are accepted only when they cover existing
/// ratchet entries. This permits `tighten deleted/path --write` to remove stale entries without
/// silently accepting typos.
pub(super) fn tighten_scope(
    root: &Path,
    paths: &[PathBuf],
    previous: &RatchetFile,
) -> Result<(RatchetScope, Vec<PathBuf>)> {
    if paths.is_empty() {
        return Ok((RatchetScope::All, Vec::new()));
    }

    let previous_paths = RatchetPaths::from_file(previous);
    let mut scopes = Vec::new();
    let mut measured_paths = Vec::new();
    for path in paths {
        let filesystem_path = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };
        let path_exists = filesystem_path.try_exists().map_err(|source| {
            sprawl_guard_lib::SprawlError::ReadFileType {
                path: filesystem_path.clone(),
                source,
            }
        })?;
        if path_exists {
            scopes.push(normalize_scope_path(root, path)?);
            measured_paths.push(path.clone());
            continue;
        }
        if path.is_absolute() {
            return Err(
                sprawl_guard_lib::SprawlError::MissingSelectedPath { path: path.clone() }.into(),
            );
        }
        let scope = RelativePath::new(path)?;
        if !previous_paths.any_at_or_under(&scope) {
            return Err(
                sprawl_guard_lib::SprawlError::MissingSelectedPath { path: path.clone() }.into(),
            );
        }
        scopes.push(scope);
    }
    Ok((RatchetScope::Paths(scopes), measured_paths))
}

fn normalize_scope_path(root: &Path, path: &Path) -> Result<RelativePath> {
    if !path.is_absolute() {
        return Ok(RelativePath::new(path)?);
    }
    let root =
        root.canonicalize()
            .map_err(|source| sprawl_guard_lib::SprawlError::ResolveRoot {
                path: root.to_path_buf(),
                source,
            })?;
    let path =
        path.canonicalize()
            .map_err(|source| sprawl_guard_lib::SprawlError::ReadFileType {
                path: path.to_path_buf(),
                source,
            })?;
    let relative =
        path.strip_prefix(&root)
            .map_err(|_| sprawl_guard_lib::SprawlError::PathOutsideRoot {
                path: path.to_path_buf(),
                root,
            })?;
    Ok(RelativePath::new(relative)?)
}

struct RatchetPaths {
    paths: Vec<RelativePath>,
}

impl RatchetPaths {
    fn from_file(file: &RatchetFile) -> Self {
        let paths = file
            .files
            .iter()
            .map(|entry| entry.path.clone())
            .chain(file.directories.iter().map(|entry| entry.path.clone()))
            .chain(file.file_sizes.iter().map(|entry| entry.path.clone()))
            .collect();
        Self { paths }
    }

    fn any_at_or_under(&self, scope: &RelativePath) -> bool {
        self.paths.iter().any(|path| path.is_at_or_under(scope))
    }
}
