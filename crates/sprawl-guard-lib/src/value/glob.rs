use std::fmt;

use globset::{Glob, GlobBuilder};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// Error returned when a glob pattern value is invalid.
#[derive(Debug, thiserror::Error)]
pub enum GlobPatternError {
    /// Glob pattern was empty.
    #[error("glob pattern must not be empty")]
    Empty,
    /// Glob pattern could not be parsed by the glob engine.
    #[error("invalid glob pattern {pattern:?}: {source}")]
    Invalid {
        /// Invalid glob pattern.
        pattern: String,
        /// Glob parser error.
        source: globset::Error,
    },
}

/// Glob pattern matched against a repository-relative path with `/` separators.
///
/// The whole path must match. `*` and `?` match within one path component, and `**` matches any
/// number of components, so `src/*.rs` matches `src/a.rs` but not `src/a/b.rs`.
#[derive(Clone, Debug, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct GlobPattern(#[schemars(length(min = 1))] String);

impl GlobPattern {
    /// Creates a glob pattern.
    pub fn new(value: impl Into<String>) -> Result<Self, GlobPatternError> {
        let value = value.into();
        if value.is_empty() {
            return Err(GlobPatternError::Empty);
        }
        parse(&value).map_err(|source| GlobPatternError::Invalid {
            pattern: value.clone(),
            source,
        })?;
        Ok(Self(value))
    }

    /// Returns the compiled form of the pattern, which every matcher is built from.
    pub(crate) fn compile(&self) -> Glob {
        parse(&self.0).expect("a glob pattern is parsed when it is created")
    }

    /// Returns the underlying pattern string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Parses a pattern with the one set of matching rules that every glob in a configuration uses.
fn parse(pattern: &str) -> Result<Glob, globset::Error> {
    GlobBuilder::new(pattern).literal_separator(true).build()
}

impl fmt::Display for GlobPattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl TryFrom<&str> for GlobPattern {
    type Error = GlobPatternError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for GlobPattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, path: &str) -> bool {
        GlobPattern::new(pattern)
            .unwrap()
            .compile()
            .compile_matcher()
            .is_match(path)
    }

    mod when_a_pattern_has_a_single_star {
        use super::*;

        #[test]
        fn it_matches_within_one_path_component() {
            assert!(matches("src/*.rs", "src/a.rs"));
            assert!(!matches("src/*.rs", "src/a/b.rs"));
            assert!(!matches("*.rs", "src/a.rs"));
        }
    }

    mod when_a_pattern_has_a_double_star {
        use super::*;

        #[test]
        fn it_matches_any_number_of_path_components() {
            assert!(matches("src/**", "src/a/b.rs"));
            assert!(matches("**/*.rs", "a.rs"));
            assert!(matches("**/*.rs", "src/a/b.rs"));
        }
    }

    mod when_a_pattern_is_a_bare_name {
        use super::*;

        #[test]
        fn it_matches_only_that_path_from_the_root() {
            assert!(matches("foo.rs", "foo.rs"));
            assert!(!matches("foo.rs", "a/foo.rs"));
        }
    }
}
