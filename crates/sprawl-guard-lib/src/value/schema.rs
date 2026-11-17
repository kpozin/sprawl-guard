use nutype::nutype;
use schemars::generate::SchemaSettings;
use schemars::transform::ReplaceConstValue;
use schemars::{Schema, SchemaGenerator};

const TOML_SCHEMA_META_SCHEMA: &str = "http://json-schema.org/draft-04/schema#";

/// Published JSON Schema location for a Sprawl Guard TOML format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TomlSchemaUrl {
    /// Version 2 root-configuration schema.
    ConfigV2,
    /// Version 1 ratchet-file schema.
    RatchetV1,
}

impl TomlSchemaUrl {
    /// Returns the URL as text.
    pub const fn as_str(self) -> &'static str {
        match self {
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

/// Renders an authored schema as compact JSON with sorted keys and a trailing newline.
///
/// Sorting keeps the bytes stable across builds that enable different `serde_json` features.
#[must_use]
pub fn render_authored_schema(schema: Schema) -> String {
    let mut document = schema.to_value();
    document.sort_all_objects();
    format!("{document}\n")
}
