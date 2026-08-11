use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// Output settings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    /// Default output format.
    pub format: OutputFormat,
    /// Color mode.
    pub color: ColorMode,
}

/// Output color mode.
#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// Detect whether color should be emitted.
    Auto,
    /// Always emit color.
    Always,
    /// Never emit color.
    Never,
}

impl<'de> Deserialize<'de> for ColorMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            value => Err(serde::de::Error::custom(format!(
                "unsupported color mode: {value}"
            ))),
        }
    }
}

/// Supported output formats.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Human-readable text output.
    Human,
    /// JSON output.
    Json,
    /// SARIF output.
    Sarif,
}

pub(super) fn merge_output(config: &mut OutputConfig, partial: PartialOutputConfig) {
    if let Some(value) = partial.format {
        config.format = value;
    }
    if let Some(value) = partial.color {
        config.color = value;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialOutputConfig {
    format: Option<OutputFormat>,
    color: Option<ColorMode>,
}
