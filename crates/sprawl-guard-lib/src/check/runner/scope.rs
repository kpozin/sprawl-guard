use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::value::path::normalize_under_root;
use crate::{RelativePath, Result, SprawlError};

/// Requested repository scope recorded in a check report.
///
/// Scope is applied during traversal and before classification, so out-of-scope trees cannot cause
/// unrelated traversal or classification errors.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckScope {
    /// Check every discovered source under the repository root.
    All,
    /// Check only the listed non-empty set of normalized path prefixes.
    Paths {
        /// Repository-relative path prefixes included in the check.
        paths: NonEmptyCheckPaths,
    },
}

/// Non-empty collection of normalized check paths.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NonEmptyCheckPaths(Vec<RelativePath>);

impl NonEmptyCheckPaths {
    fn new(paths: Vec<RelativePath>) -> Option<Self> {
        if paths.is_empty() {
            return None;
        }
        Some(Self(paths))
    }

    /// Returns the normalized repository-relative path prefixes.
    pub fn as_slice(&self) -> &[RelativePath] {
        &self.0
    }
}

impl CheckScope {
    /// Resolves CLI-supplied path scopes under the repository root.
    ///
    /// Every non-empty scope must exist so a typo cannot produce a passing empty check.
    pub(super) fn resolve(root: &Path, paths: Vec<PathBuf>) -> Result<Self> {
        let paths = paths
            .into_iter()
            .map(|path| scope_path(root, &path))
            .collect::<Result<Vec<_>>>()?;
        if paths.iter().any(|path| path.as_str().is_empty()) {
            return Ok(Self::All);
        }
        Ok(match NonEmptyCheckPaths::new(paths) {
            Some(paths) => Self::Paths { paths },
            None => Self::All,
        })
    }

    /// Returns normalized path scopes for traversal pruning.
    pub(super) fn paths(&self) -> &[RelativePath] {
        match self {
            Self::All => &[],
            Self::Paths { paths } => paths.as_slice(),
        }
    }

    /// Returns whether the request covers the whole repository.
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    /// Returns whether a repository-relative path falls within this scope.
    pub(in crate::check) fn contains(&self, path: &RelativePath) -> bool {
        self.is_all() || self.paths().iter().any(|scope| path.is_at_or_under(scope))
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
