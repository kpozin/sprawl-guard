use std::fmt;
use std::str::FromStr;

use nutype::nutype;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};

/// Error returned when a custom language-group name is invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LanguageGroupIdError {
    /// The custom name was empty.
    #[error("language-group name must not be empty")]
    Empty,
    /// The custom name did not start with an ASCII lowercase letter.
    #[error("language-group name must start with an ASCII lowercase letter: {name}")]
    InvalidStart {
        /// Invalid custom name.
        name: String,
    },
    /// The custom name contained a character outside lowercase ASCII snake case.
    #[error("language-group name contains invalid character {character:?}: {name}")]
    InvalidCharacter {
        /// Invalid custom name.
        name: String,
        /// Invalid character.
        character: char,
    },
    /// The custom name was reserved for an automatic language group.
    #[error("language-group name is reserved for an automatic group: {name}")]
    Reserved {
        /// Reserved automatic group name.
        name: String,
    },
}

fn validate_custom_language_group_id(value: &str) -> Result<(), LanguageGroupIdError> {
    let Some(first) = value.chars().next() else {
        return Err(LanguageGroupIdError::Empty);
    };
    if !first.is_ascii_lowercase() {
        return Err(LanguageGroupIdError::InvalidStart {
            name: value.to_owned(),
        });
    }
    if let Some(character) = value.chars().skip(1).find(|character| {
        !(character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '_')
    }) {
        return Err(LanguageGroupIdError::InvalidCharacter {
            name: value.to_owned(),
            character,
        });
    }
    if matches!(value, "source" | "documentation") {
        return Err(LanguageGroupIdError::Reserved {
            name: value.to_owned(),
        });
    }
    Ok(())
}

/// Validated custom language-group name.
#[nutype(
    validate(with = validate_custom_language_group_id, error = LanguageGroupIdError),
    derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, AsRef, Display),
)]
pub struct CustomLanguageGroupId(String);

impl Serialize for CustomLanguageGroupId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}

/// Identifier for a language group used by configuration, results, and ratchets.
///
/// Custom deserialization maps automatic names to their explicit variants and
/// validates every other name before creating a custom payload.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum LanguageGroupId {
    /// Automatic group for ordinary source languages.
    Source,
    /// Automatic group for Tokei literate languages.
    Documentation,
    /// User-defined group with a validated custom name.
    Custom(CustomLanguageGroupId),
}

impl JsonSchema for LanguageGroupId {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "LanguageGroupId".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "anyOf": [
                { "enum": ["source", "documentation"], "type": "string" },
                { "pattern": "^[a-z][a-z0-9_]*$", "type": "string" }
            ],
            "title": "Language group ID"
        })
    }
}

impl LanguageGroupId {
    /// Creates a language-group identifier for a custom group.
    pub fn custom(value: impl Into<String>) -> Result<Self, LanguageGroupIdError> {
        CustomLanguageGroupId::try_new(value).map(Self::Custom)
    }

    /// Returns the canonical serialized identifier.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Source => "source",
            Self::Documentation => "documentation",
            Self::Custom(name) => name.as_ref(),
        }
    }
}

impl fmt::Display for LanguageGroupId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for LanguageGroupId {
    type Error = LanguageGroupIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for LanguageGroupId {
    type Err = LanguageGroupIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "source" => Ok(Self::Source),
            "documentation" => Ok(Self::Documentation),
            _ => Self::custom(value),
        }
    }
}

impl Serialize for LanguageGroupId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LanguageGroupId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{LanguageGroupId, LanguageGroupIdError};

    mod when_creating_a_custom_language_group {
        use super::*;

        #[test]
        fn it_accepts_lowercase_ascii_snake_case_names() {
            for name in ["frontend", "web_2", "x"] {
                assert_eq!(LanguageGroupId::custom(name).unwrap().to_string(), name);
            }
        }

        #[test]
        fn it_rejects_an_empty_name() {
            assert_eq!(
                LanguageGroupId::custom("").unwrap_err(),
                LanguageGroupIdError::Empty,
            );
        }

        #[test]
        fn it_rejects_a_name_that_starts_with_a_digit_or_underscore() {
            for name in ["2frontend", "_frontend"] {
                assert_eq!(
                    LanguageGroupId::custom(name).unwrap_err(),
                    LanguageGroupIdError::InvalidStart {
                        name: name.to_owned(),
                    },
                );
            }
        }

        #[test]
        fn it_rejects_names_with_non_snake_case_characters() {
            for (name, character) in [("front-end", '-'), ("fronté", 'é')] {
                assert_eq!(
                    LanguageGroupId::custom(name).unwrap_err(),
                    LanguageGroupIdError::InvalidCharacter {
                        name: name.to_owned(),
                        character,
                    },
                );
            }
        }

        #[test]
        fn it_rejects_automatic_group_names() {
            for name in ["source", "documentation"] {
                assert_eq!(
                    LanguageGroupId::custom(name).unwrap_err(),
                    LanguageGroupIdError::Reserved {
                        name: name.to_owned(),
                    },
                );
            }
        }
    }

    mod when_serializing_a_language_group {
        use super::*;

        #[test]
        fn it_uses_the_canonical_automatic_or_custom_name() {
            let groups = [
                (LanguageGroupId::Source, r#""source""#),
                (LanguageGroupId::Documentation, r#""documentation""#),
                (
                    LanguageGroupId::custom("frontend").unwrap(),
                    r#""frontend""#,
                ),
            ];

            for (group, expected_json) in groups {
                assert_eq!(serde_json::to_string(&group).unwrap(), expected_json);
            }
        }
    }

    mod when_deserializing_a_language_group {
        use super::*;

        #[test]
        fn it_recovers_automatic_or_valid_custom_variants() {
            let groups = [
                (r#""source""#, LanguageGroupId::Source),
                (r#""documentation""#, LanguageGroupId::Documentation),
                (
                    r#""frontend_2""#,
                    LanguageGroupId::custom("frontend_2").unwrap(),
                ),
            ];

            for (json, expected) in groups {
                assert_eq!(
                    serde_json::from_str::<LanguageGroupId>(json).unwrap(),
                    expected
                );
            }
        }

        #[test]
        fn it_rejects_invalid_custom_names() {
            let error = serde_json::from_str::<LanguageGroupId>(r#""2frontend""#).unwrap_err();

            assert!(
                error
                    .to_string()
                    .contains("must start with an ASCII lowercase letter")
            );
        }
    }
}
