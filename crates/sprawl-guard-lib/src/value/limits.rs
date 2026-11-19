use std::borrow::Cow;
use std::fmt;
use std::num::{NonZeroU32, ParseIntError};
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Finite or unlimited limit value.
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

/// Parses command-line text: `unlimited`, or whatever the finite value accepts.
impl<Value> FromStr for FiniteOrUnlimited<Value>
where
    Value: FromStr,
{
    type Err = Value::Err;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == "unlimited" {
            Ok(Self::Unlimited)
        } else {
            text.parse().map(Self::Finite)
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
    /// A configured limit was zero, but limits must be positive.
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
    Lines,
    /// Maximum count of direct files.
    DirectFiles,
    /// Maximum count of findings listed in a report.
    Findings,
}

impl fmt::Display for LimitValueKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lines => formatter.write_str("line limit"),
            Self::DirectFiles => formatter.write_str("leaf-file count"),
            Self::Findings => formatter.write_str("finding cap"),
        }
    }
}

/// Positive line limit.
#[derive(Clone, Copy, Debug, Eq, Ord, JsonSchema, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct LineLimit(NonZeroU32);

impl LineLimit {
    /// Creates a line limit.
    pub fn new(value: u32) -> Result<Self, LimitValueError> {
        let Some(value) = NonZeroU32::new(value) else {
            return Err(LimitValueError::Zero {
                kind: LimitValueKind::Lines,
            });
        };
        Ok(Self(value))
    }

    /// Creates a line limit from a trusted built-in value.
    pub(crate) const fn trusted(value: u32) -> Self {
        let Some(value) = NonZeroU32::new(value) else {
            panic!("trusted line limit must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw count.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for LineLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Count of direct files.
#[derive(Clone, Copy, Debug, Eq, Ord, JsonSchema, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct DirectFiles(NonZeroU32);

impl DirectFiles {
    /// Creates a direct-file count.
    pub fn new(value: u32) -> Result<Self, LimitValueError> {
        let Some(value) = NonZeroU32::new(value) else {
            return Err(LimitValueError::Zero {
                kind: LimitValueKind::DirectFiles,
            });
        };
        Ok(Self(value))
    }

    /// Creates a direct-file count from a trusted built-in value.
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

impl fmt::Display for DirectFiles {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Finite or unlimited direct-file limit.
pub type DirectFileLimit = FiniteOrUnlimited<DirectFiles>;

impl<'de> Deserialize<'de> for LineLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for DirectFiles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Positive number of findings that the human report lists.
///
/// Findings past this number are counted in their group's total but not listed, so a large run
/// keeps its summary in view.
#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MaxFindings(NonZeroU32);

impl MaxFindings {
    /// Creates a finding cap.
    pub fn new(value: u32) -> Result<Self, LimitValueError> {
        NonZeroU32::new(value)
            .map(Self)
            .ok_or(LimitValueError::Zero {
                kind: LimitValueKind::Findings,
            })
    }

    /// Creates a finding cap from a trusted built-in value.
    pub(crate) const fn trusted(value: u32) -> Self {
        let Some(value) = NonZeroU32::new(value) else {
            panic!("trusted finding cap must be greater than zero");
        };
        Self(value)
    }

    /// Returns the raw count.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for MaxFindings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for MaxFindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Error returned when command-line text is not a valid finding cap.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum ParseMaxFindingsError {
    /// The text is not a non-negative integer that fits in 32 bits.
    #[error("expected a positive integer or \"unlimited\"")]
    NotAnInteger(#[source] ParseIntError),
    /// The text is zero.
    #[error(transparent)]
    Invalid(#[from] LimitValueError),
}

impl FromStr for MaxFindings {
    type Err = ParseMaxFindingsError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let value = text.parse().map_err(ParseMaxFindingsError::NotAnInteger)?;
        Ok(Self::new(value)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod when_parsing_command_line_text {
        use super::*;

        #[test]
        fn it_accepts_a_positive_integer() {
            assert_eq!(
                "20".parse::<FiniteOrUnlimited<MaxFindings>>(),
                Ok(FiniteOrUnlimited::Finite(MaxFindings::trusted(20)))
            );
        }

        #[test]
        fn it_accepts_unlimited() {
            assert_eq!(
                "unlimited".parse::<FiniteOrUnlimited<MaxFindings>>(),
                Ok(FiniteOrUnlimited::Unlimited)
            );
        }

        #[test]
        fn it_rejects_zero() {
            assert_eq!(
                "0".parse::<MaxFindings>().unwrap_err().to_string(),
                "finding cap must be greater than zero"
            );
        }

        #[test]
        fn it_rejects_other_text() {
            assert_eq!(
                "all".parse::<MaxFindings>().unwrap_err().to_string(),
                "expected a positive integer or \"unlimited\""
            );
        }
    }
}
