use nutype::nutype;
use schemars::generate::SchemaSettings;
use schemars::transform::{ReplaceConstValue, Transform, transform_subschemas};
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
            settings.transforms.push(Box::new(ReplaceIntegerFormats));
        })
        .for_deserialize()
        .into_generator()
}

/// Replaces the `format` that schemars gives integer types, such as `uint32` for `u32`, with
/// standard constraints.
///
/// These formats aren't part of JSON Schema, so strict validators such as SchemaStore's reject the
/// schema. A `u32` gets an explicit `maximum` so editors flag values that don't fit. A `u64` gets
/// none, because its maximum can't be written exactly as a JSON number; the TOML parser still
/// rejects values that don't fit. Any other integer format panics, so a new integer type can't
/// lose its bound without notice.
#[derive(Clone, Copy, Debug)]
struct ReplaceIntegerFormats;

impl Transform for ReplaceIntegerFormats {
    fn transform(&mut self, schema: &mut Schema) {
        // `type` is a name, or a list of names when the value can also be null.
        let types = schema.get("type");
        let is_integer = types.and_then(|value| value.as_str()) == Some("integer")
            || types
                .and_then(|value| value.as_array())
                .is_some_and(|names| names.iter().any(|name| name == "integer"));
        if is_integer && let Some(format) = schema.remove("format") {
            match format.as_str() {
                Some("uint32") => {
                    schema.insert("maximum".into(), u32::MAX.into());
                }
                Some("uint64") => {}
                _ => panic!("add a JSON Schema bound for the integer format {format}"),
            }
        }
        transform_subschemas(self, schema);
    }
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

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize, JsonSchema)]
    #[expect(dead_code, reason = "only the schema is under test")]
    struct Limits {
        lines: u32,
        files: Option<u32>,
        bytes: Option<u64>,
    }

    mod when_a_type_has_unsigned_integer_fields {
        use super::*;

        #[test]
        fn its_schema_names_no_format_and_bounds_each_u32() {
            let schema = authored_toml_schema_generator()
                .into_root_schema_for::<Limits>()
                .to_value();

            assert!(!schema.to_string().contains("\"format\""), "{schema}");
            assert_eq!(schema["properties"]["lines"]["minimum"], 0);
            assert_eq!(schema["properties"]["lines"]["maximum"], u32::MAX);
            assert_eq!(schema["properties"]["files"]["maximum"], u32::MAX);
            assert_eq!(schema["properties"]["bytes"]["minimum"], 0);
            assert!(
                schema["properties"]["bytes"].get("maximum").is_none(),
                "{schema}"
            );
        }
    }
}
