use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{FiniteOrUnlimited, MaxFindings};

/// Output settings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    /// Default output format.
    pub format: OutputFormat,
    /// How many findings the human report lists before summarizing the rest.
    pub max_findings: FiniteOrUnlimited<MaxFindings>,
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
    /// Human-readable text output, followed by GitHub Actions annotations.
    Github,
}

pub(super) fn merge_output(config: &mut OutputConfig, partial: PartialOutputConfig) {
    if let Some(value) = partial.format {
        config.format = value;
    }
    if let Some(value) = partial.max_findings {
        config.max_findings = value;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialOutputConfig {
    pub(super) format: Option<OutputFormat>,
    pub(super) max_findings: Option<FiniteOrUnlimited<MaxFindings>>,
}
