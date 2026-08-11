use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::SprawlError;

/// Supported root configuration schema versions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "i64", into = "i64")]
pub enum ConfigSchemaVersion {
    /// Configuration schema version 1.
    V1,
}

impl JsonSchema for ConfigSchemaVersion {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ConfigSchemaVersion".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "const": 1,
            "title": "Configuration schema version",
            "type": "integer"
        })
    }
}

impl ConfigSchemaVersion {
    /// Schema version emitted by this build.
    pub const CURRENT: Self = Self::V1;
}

impl From<ConfigSchemaVersion> for i64 {
    fn from(version: ConfigSchemaVersion) -> Self {
        match version {
            ConfigSchemaVersion::V1 => 1,
        }
    }
}

impl TryFrom<i64> for ConfigSchemaVersion {
    type Error = SprawlError;

    fn try_from(version: i64) -> Result<Self, Self::Error> {
        match version {
            1 => Ok(Self::V1),
            version => Err(SprawlError::UnsupportedConfigSchemaVersion { version }),
        }
    }
}
