use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};

use crate::value::path::normalize_under_root;
use crate::{RelativePath, Result, SprawlError};

/// The paths a check covers: the whole repository or the selected paths.
///
/// Selection is applied during traversal and before classification, so unselected trees cannot
/// cause unrelated traversal or classification errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectedPaths {
    /// Check every discovered source under the repository root.
    All,
    /// Check only the listed non-empty set of normalized path prefixes.
    Paths {
        /// Repository-relative path prefixes included in the check.
        paths: NonEmptyCheckPaths,
    },
}

/// Serializes the whole repository as `"all"` and a selection as its list of paths.
impl Serialize for SelectedPaths {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::All => serializer.serialize_str(ALL_PATHS),
            Self::Paths { paths } => paths.serialize(serializer),
        }
    }
}

/// Serialized form of [`SelectedPaths::All`].
const ALL_PATHS: &str = "all";

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

impl SelectedPaths {
    /// Resolves CLI-supplied selected paths under the repository root.
    ///
    /// Every non-empty selected path must exist so a typo cannot produce a passing empty check.
    pub(super) fn resolve(root: &Path, paths: Vec<PathBuf>) -> Result<Self> {
        let paths = paths
            .into_iter()
            .map(|path| selected_path(root, &path))
            .collect::<Result<Vec<_>>>()?;
        if paths.iter().any(|path| path.as_str().is_empty()) {
            return Ok(Self::All);
        }
        Ok(match NonEmptyCheckPaths::new(paths) {
            Some(paths) => Self::Paths { paths },
            None => Self::All,
        })
    }

    /// Returns normalized selected paths for traversal pruning.
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

    /// Returns whether a repository-relative path falls within these selected paths.
    pub(in crate::check) fn contains(&self, path: &RelativePath) -> bool {
        self.is_all()
            || self
                .paths()
                .iter()
                .any(|selected| path.is_at_or_under(selected))
    }
}

fn selected_path(root: &Path, path: &Path) -> Result<RelativePath> {
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
        return Err(SprawlError::MissingSelectedPath {
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
