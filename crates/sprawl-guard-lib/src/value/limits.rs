use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroU32;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Finite or unlimited policy value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FiniteOrUnlimited<Value> {
    /// Enforce the contained value.
    Finite(Value),
    /// Measure without enforcing a ceiling.
    Unlimited,
}

impl<Value> JsonSchema for FiniteOrUnlimited<Value>
where
    Value: JsonSchema,
{
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        format!("FiniteOrUnlimited_{}", Value::schema_name()).into()
    }

    fn schema_id() -> Cow<'static, str> {
        format!("FiniteOrUnlimited<{}>", Value::schema_id()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "anyOf": [
                generator.subschema_for::<Value>(),
                {
                    "const": "unlimited",
                    "type": "string"
                }
            ]
        })
    }
}

impl<Value> FiniteOrUnlimited<Value>
where
    Value: Copy,
{
    /// Returns the finite limit, or `None` when the metric is unlimited.
    pub const fn finite(self) -> Option<Value> {
        match self {
            Self::Finite(value) => Some(value),
            Self::Unlimited => None,
        }
    }
}

impl<Value> fmt::Display for FiniteOrUnlimited<Value>
where
    Value: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Finite(value) => value.fmt(formatter),
            Self::Unlimited => formatter.write_str("unlimited"),
        }
    }
}

impl<Value> Serialize for FiniteOrUnlimited<Value>
where
    Value: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Finite(value) => value.serialize(serializer),
            Self::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de, Value> Deserialize<'de> for FiniteOrUnlimited<Value>
where
    Value: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RawFiniteOrUnlimited::deserialize(deserializer)? {
            RawFiniteOrUnlimited::Finite(value) => Ok(Self::Finite(value)),
            RawFiniteOrUnlimited::Keyword(value) if value == "unlimited" => Ok(Self::Unlimited),
            RawFiniteOrUnlimited::Keyword(value) => Err(serde::de::Error::custom(format!(
                "unsupported limit {value:?}; expected a positive integer or \"unlimited\"",
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawFiniteOrUnlimited<Value> {
    Finite(Value),
    Keyword(String),
}

/// Error returned when a configured limit value is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LimitValueError {
    /// A configured limit was zero, but policy limits must be positive.
    #[error("{kind} must be greater than zero")]
    Zero {
        /// Limit kind that received the invalid value.
        kind: LimitValueKind,
    },
}

/// Kind of configured limit value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitValueKind {
    /// Maximum configured lines of code.
    Loc,
    /// Maximum count of direct leaf files.
    LeafFiles,
}

impl fmt::Display for LimitValueKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loc => formatter.write_str("LOC limit"),
            Self::LeafFiles => formatter.write_str("leaf-file count"),
        }
    }
}

/// Positive configured LOC limit.
#[derive(Clone, Copy, Debug, Eq, Ord, JsonSchema, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct LocLimit(NonZeroU32);

impl LocLimit {
    /// Creates a configured LOC limit.
    pub fn new(value: u32) -> Result<Self, LimitValueError> {
        let Some(value) = NonZeroU32::new(value) else {
            return Err(LimitValueError::Zero {
                kind: LimitValueKind::Loc,
            });
        };
        Ok(Self(value))
    }

    /// Creates a configured LOC limit from a trusted built-in value.
    pub(crate) const fn trusted(value: u32) -> Self {
        let Some(value) = NonZeroU32::new(value) else {
            panic!("trusted LOC limit must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw count.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for LocLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Count of direct leaf files.
#[derive(Clone, Copy, Debug, Eq, Ord, JsonSchema, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct LeafFiles(NonZeroU32);

impl LeafFiles {
    /// Creates a leaf-file count.
    pub fn new(value: u32) -> Result<Self, LimitValueError> {
        let Some(value) = NonZeroU32::new(value) else {
            return Err(LimitValueError::Zero {
                kind: LimitValueKind::LeafFiles,
            });
        };
        Ok(Self(value))
    }

    /// Creates a leaf-file count from a trusted built-in value.
    pub(crate) const fn trusted(value: u32) -> Self {
        let Some(value) = NonZeroU32::new(value) else {
            panic!("trusted leaf-file count must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw count.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for LeafFiles {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Finite or unlimited direct leaf-file budget.
pub type LeafFileLimit = FiniteOrUnlimited<LeafFiles>;

impl<'de> Deserialize<'de> for LocLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for LeafFiles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
