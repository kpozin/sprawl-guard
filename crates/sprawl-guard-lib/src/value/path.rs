use std::fmt;
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{Result, SprawlError};

/// Normalized repository-relative path using forward slash separators.
#[derive(Clone, Debug, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct RelativePath(#[schemars(pattern(r"^(?!/)(?!.*(?:^|/)\.\.(?:/|$)).*$"))] String);

impl RelativePath {
    /// Normalizes a UTF-8 path that must be relative and must not traverse upward.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.is_absolute() {
            return Err(SprawlError::AbsolutePath {
                path: path.to_path_buf(),
            });
        }

        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => {
                    let Some(part) = part.to_str() else {
                        return Err(SprawlError::NonUtf8Path {
                            path: path.to_path_buf(),
                        });
                    };
                    parts.push(part.to_owned());
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(SprawlError::ParentPath {
                        path: path.to_path_buf(),
                    });
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(SprawlError::AbsolutePath {
                        path: path.to_path_buf(),
                    });
                }
            }
        }

        Ok(Self(parts.join("/")))
    }

    /// Returns the normalized path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns whether this path is exactly the scope or under it.
    ///
    /// Empty scopes represent the repository root.
    pub fn is_at_or_under(&self, scope: &Self) -> bool {
        scope.as_str().is_empty()
            || self.as_str() == scope.as_str()
            || self
                .as_str()
                .strip_prefix(scope.as_str())
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    /// Converts this path to a platform-native `PathBuf`.
    pub fn to_path_buf(&self) -> PathBuf {
        self.0.split('/').collect()
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Normalizes a root-relative path after stripping a known root prefix.
pub fn normalize_under_root(root: &Path, path: &Path) -> Result<RelativePath> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| SprawlError::PathOutsideRoot {
            path: path.to_path_buf(),
            root: root.to_path_buf(),
        })?;
    RelativePath::new(relative)
}

/// Resolves a configured repository root.
pub(crate) fn resolve_root(root: &Path) -> Result<PathBuf> {
    root.canonicalize()
        .map_err(|source| SprawlError::ResolveRoot {
            path: root.to_path_buf(),
            source,
        })
}

/// Repository-relative ratchet file path.
#[derive(Clone, Debug, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct RatchetFilePath(#[schemars(length(min = 1), pattern(r"^[^\\:]+$"))] RelativePath);

/// Error returned when a ratchet file path is not repository-relative.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RatchetFilePathError {
    /// Path was empty.
    #[error("ratchet file path must not be empty")]
    Empty,
    /// Path used backslash separators.
    #[error("ratchet file path must use normalized forward slash separators: {path}")]
    BackslashSeparator {
        /// Invalid path.
        path: PathBuf,
    },
    /// Path contained a colon.
    #[error("ratchet file path must not contain ':' characters: {path}")]
    Colon {
        /// Invalid path.
        path: PathBuf,
    },
    /// Path was absolute.
    #[error("ratchet file path must be relative: {path}")]
    Absolute {
        /// Invalid path.
        path: PathBuf,
    },
    /// Path used parent-directory traversal.
    #[error("ratchet file path must not contain '..': {path}")]
    Parent {
        /// Invalid path.
        path: PathBuf,
    },
}

impl RatchetFilePath {
    /// Creates a ratchet file path.
    pub fn new(value: impl Into<String>) -> std::result::Result<Self, RatchetFilePathError> {
        let value = value.into();
        validate_ratchet_file_path(&value)?;
        let path = RelativePath::new(&value)
            .expect("ratchet path validation must reject every invalid relative path");
        if path.as_str().is_empty() {
            return Err(RatchetFilePathError::Empty);
        }
        Ok(Self(path))
    }

    /// Creates a ratchet file path from a trusted built-in value.
    pub(crate) fn trusted(value: &'static str) -> Self {
        Self::new(value).expect("built-in ratchet file path must be valid")
    }

    /// Returns the normalized path string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for RatchetFilePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl TryFrom<&str> for RatchetFilePath {
    type Error = RatchetFilePathError;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for RatchetFilePath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn validate_ratchet_file_path(value: &str) -> std::result::Result<(), RatchetFilePathError> {
    if value.is_empty() {
        return Err(RatchetFilePathError::Empty);
    }
    if value.contains('\\') {
        return Err(RatchetFilePathError::BackslashSeparator {
            path: PathBuf::from(value),
        });
    }
    if value.contains(':') {
        return Err(RatchetFilePathError::Colon {
            path: PathBuf::from(value),
        });
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(RatchetFilePathError::Absolute {
            path: PathBuf::from(value),
        });
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(RatchetFilePathError::Parent {
                    path: PathBuf::from(value),
                });
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(RatchetFilePathError::Absolute {
                    path: PathBuf::from(value),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod relative_path {
        use super::*;

        mod when_path_contains_current_directory {
            use super::*;

            #[test]
            fn it_removes_current_directory_components() {
                let path = RelativePath::new("./src/./main.rs").unwrap();

                assert_eq!(path.as_str(), "src/main.rs");
            }
        }

        mod when_path_contains_parent_directory {
            use super::*;

            #[test]
            fn it_rejects_the_path() {
                let error = RelativePath::new("src/../main.rs").unwrap_err();

                assert!(error.to_string().contains("must not contain '..'"));
            }
        }

        mod when_path_is_absolute {
            use super::*;

            #[test]
            fn it_rejects_the_path() {
                let error = RelativePath::new("/tmp/main.rs").unwrap_err();

                assert!(error.to_string().contains("must be relative"));
            }
        }

        #[cfg(unix)]
        mod when_path_is_not_utf8 {
            use std::ffi::OsString;
            use std::os::unix::ffi::OsStringExt;

            use super::*;

            #[test]
            fn it_rejects_the_path_without_lossy_normalization() {
                let path = PathBuf::from(OsString::from_vec(vec![b's', b'r', b'c', b'/', 0xff]));

                let error = RelativePath::new(&path).unwrap_err();

                assert!(matches!(error, SprawlError::NonUtf8Path { .. }));
            }
        }

        mod when_checking_scope_containment {
            use super::*;

            #[test]
            fn it_matches_the_same_path() {
                let path = RelativePath::new("src/lib.rs").unwrap();
                let scope = RelativePath::new("src/lib.rs").unwrap();

                assert!(path.is_at_or_under(&scope));
            }

            #[test]
            fn it_matches_a_descendant_path() {
                let path = RelativePath::new("src/lib.rs").unwrap();
                let scope = RelativePath::new("src").unwrap();

                assert!(path.is_at_or_under(&scope));
            }

            #[test]
            fn it_treats_empty_scope_as_the_repository_root() {
                let path = RelativePath::new("src/lib.rs").unwrap();
                let scope = RelativePath::new("").unwrap();

                assert!(path.is_at_or_under(&scope));
            }

            #[test]
            fn it_rejects_plain_string_prefixes() {
                let path = RelativePath::new("src-lib.rs").unwrap();
                let scope = RelativePath::new("src").unwrap();

                assert!(!path.is_at_or_under(&scope));
            }
        }

        mod when_deserializing {
            use super::*;

            #[test]
            fn it_accepts_normalized_relative_paths() {
                let path: RelativePath = serde_json::from_str("\"src/main.rs\"").unwrap();

                assert_eq!(path.as_str(), "src/main.rs");
            }

            #[test]
            fn it_rejects_parent_directory_paths() {
                let error = serde_json::from_str::<RelativePath>("\"../main.rs\"").unwrap_err();

                assert!(error.to_string().contains("must not contain '..'"));
            }

            #[test]
            fn it_rejects_absolute_paths() {
                let error = serde_json::from_str::<RelativePath>("\"/tmp/main.rs\"").unwrap_err();

                assert!(error.to_string().contains("must be relative"));
            }
        }
    }

    mod ratchet_file_path {
        use super::*;

        mod when_path_has_redundant_components {
            use super::*;

            #[test]
            fn it_stores_the_canonical_repository_relative_path() {
                let path = RatchetFilePath::new("./nested//ratchet.toml").unwrap();

                assert_eq!(path.as_str(), "nested/ratchet.toml");
            }
        }

        mod when_path_uses_windows_reserved_name {
            use super::*;

            #[test]
            fn it_accepts_the_normalized_path_without_platform_specific_filtering() {
                let path = RatchetFilePath::new("CON.toml").unwrap();

                assert_eq!(path.as_str(), "CON.toml");
            }
        }

        mod when_path_is_long {
            use super::*;

            #[test]
            fn it_accepts_the_normalized_path_without_a_windows_specific_length_limit() {
                let component = "nested".repeat(40);
                let value = format!("{component}/{component}/ratchet.toml");
                let path = RatchetFilePath::new(&value).unwrap();

                assert_eq!(path.as_str(), value);
            }
        }
    }
}
