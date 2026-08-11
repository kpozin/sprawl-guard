use std::path::{Path, PathBuf};

use nutype::nutype;

use crate::value::path::normalize_under_root;
use crate::{RelativePath, Result, SprawlError};

/// Normalized set of path prefixes that limit which files a check considers.
///
/// A scope is applied during traversal and before classification, so out-of-scope trees cannot
/// cause unrelated traversal or classification errors. An empty list covers the whole repository.
#[nutype(derive(AsRef))]
pub(in crate::check) struct CheckScope(Vec<RelativePath>);

impl CheckScope {
    /// Resolves CLI-supplied path scopes under the repository root.
    ///
    /// Every non-empty scope must exist so a typo cannot produce a passing empty check.
    pub(super) fn resolve(root: &Path, paths: Vec<PathBuf>) -> Result<Self> {
        let paths = paths
            .into_iter()
            .map(|path| scope_path(root, &path))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self::new(paths))
    }

    /// Returns normalized path scopes for traversal pruning.
    pub(super) fn paths(&self) -> &[RelativePath] {
        self.as_ref().as_slice()
    }

    /// Returns whether a repository-relative path falls within this scope.
    pub(in crate::check) fn contains(&self, path: &RelativePath) -> bool {
        self.paths().is_empty() || self.paths().iter().any(|scope| path.is_at_or_under(scope))
    }
}

fn scope_path(root: &Path, path: &Path) -> Result<RelativePath> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if !absolute_path
        .try_exists()
        .map_err(|source| SprawlError::ReadFileType {
            path: absolute_path.clone(),
            source,
        })?
    {
        return Err(SprawlError::MissingCheckScope {
            path: path.to_path_buf(),
        });
    }
    if path.is_absolute() {
        let canonical_path =
            absolute_path
                .canonicalize()
                .map_err(|source| SprawlError::ReadFileType {
                    path: absolute_path.clone(),
                    source,
                })?;
        return normalize_under_root(root, &canonical_path);
    }
    RelativePath::new(path)
}
