use nutype::nutype;
use schemars::SchemaGenerator;
use schemars::generate::SchemaSettings;
use schemars::transform::ReplaceConstValue;

const TOML_SCHEMA_META_SCHEMA: &str = "http://json-schema.org/draft-04/schema#";

/// Published JSON Schema location for a Sprawl Guard TOML format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TomlSchemaUrl {
    /// Version 1 root-configuration schema.
    ConfigV1,
    /// Version 2 root-configuration schema.
    ConfigV2,
    /// Version 1 ratchet-file schema.
    RatchetV1,
}

impl TomlSchemaUrl {
    /// Returns the URL as text.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigV1 => {
                "https://kpozin.github.io/sprawl-guard/schemas/v1/sprawl-guard.schema.json"
            }
            Self::ConfigV2 => {
                "https://kpozin.github.io/sprawl-guard/schemas/v2/sprawl-guard.schema.json"
            }
            Self::RatchetV1 => concat!(
                "https://kpozin.github.io/sprawl-guard/schemas/v1/",
                "sprawl-guard.ratchet.schema.json"
            ),
        }
    }
}

impl std::fmt::Display for TomlSchemaUrl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Taplo directive that selects a published Sprawl Guard TOML schema.
#[nutype(derive(AsRef, Clone, Copy, Debug, Eq, PartialEq))]
pub struct TomlSchemaDirective(TomlSchemaUrl);

impl std::fmt::Display for TomlSchemaDirective {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "#:schema {}", self.as_ref())
    }
}

/// Builds the common schema generator for Sprawl Guard's authored TOML formats.
pub(crate) fn authored_toml_schema_generator() -> SchemaGenerator {
    SchemaSettings::draft07()
        .with(|settings| {
            settings.meta_schema = Some(TOML_SCHEMA_META_SCHEMA.into());
            settings
                .transforms
                .push(Box::new(ReplaceConstValue::default()));
        })
        .for_deserialize()
        .into_generator()
}
