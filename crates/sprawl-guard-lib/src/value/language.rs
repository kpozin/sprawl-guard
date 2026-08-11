use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};
use tokei::LanguageType;

use super::language_group::LanguageGroupId;
use super::line_types::IncludedLineTypes;

/// Error returned when a language identifier is not supported by `tokei`.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LanguageIdError {
    /// Language identifier is not in `tokei`'s supported language set.
    #[error("unsupported language ID: {language}")]
    Unsupported {
        /// Unsupported language identifier.
        language: String,
    },
}

/// Language identifier backed by `tokei`'s supported language set.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct LanguageId(LanguageType);

impl JsonSchema for LanguageId {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "LanguageId".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        // The enum supplies documented lowercase completions. Tokei also accepts aliases and
        // case-insensitive spellings, which remain strings for the runtime parser to validate.
        let languages = LanguageType::list()
            .iter()
            .map(|(language, _)| language.name().to_lowercase())
            .collect::<std::collections::BTreeSet<_>>();
        json_schema!({
            "anyOf": [
                {
                    "enum": languages,
                    "type": "string"
                },
                {
                    "type": "string"
                }
            ],
            "title": "Language ID",
        })
    }
}

impl LanguageId {
    /// Creates a language identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, LanguageIdError> {
        let value = value.into();
        LanguageType::from_str(&value)
            .map(Self)
            .map_err(|_| LanguageIdError::Unsupported { language: value })
    }

    /// Creates a language identifier from a `tokei` language.
    pub(crate) const fn from_tokei(language: LanguageType) -> Self {
        Self(language)
    }

    /// Returns the underlying `tokei` language.
    pub const fn as_tokei(self) -> LanguageType {
        self.0
    }

    /// Reports whether the language treats prose as a logical comment.
    pub fn is_literate(self) -> bool {
        self.0.is_literate()
    }

    /// Returns the automatic directory fan-out group for this language.
    pub fn automatic_group(self) -> LanguageGroupId {
        if self.is_literate() {
            LanguageGroupId::Documentation
        } else {
            LanguageGroupId::Source
        }
    }

    /// Applies the automatic literate-language policy to global included line types.
    pub fn automatic_included_line_types(self, global: IncludedLineTypes) -> IncludedLineTypes {
        if self.is_literate() {
            global.with_comments()
        } else {
            global
        }
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl TryFrom<&str> for LanguageId {
    type Error = LanguageIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for LanguageId {
    type Err = LanguageIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl Serialize for LanguageId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.name())
    }
}

impl<'de> Deserialize<'de> for LanguageId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
