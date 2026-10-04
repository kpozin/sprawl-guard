use serde_json::Value;
use sprawl_guard_lib::authored_ratchet_json_schema;
use sprawl_guard_lib::config::authored_config_json_schema;

use super::SchemaDocumentKind;
use crate::error::{CliError, Result};

/// Generates the selected authored-TOML JSON Schema payload.
pub(super) fn schema_payload(format: SchemaDocumentKind) -> Result<Value> {
    let schema = match format {
        SchemaDocumentKind::Config => authored_config_json_schema(),
        SchemaDocumentKind::Ratchet => authored_ratchet_json_schema(),
    };
    serde_json::to_value(schema).map_err(|source| CliError::RenderJson { source })
}

/// Renders a generated schema as compact JSON for direct distribution.
pub(super) fn render_schema_stdout(payload: &Value) -> Result<String> {
    serde_json::to_string(payload)
        .map(|output| format!("{output}\n"))
        .map_err(|source| CliError::RenderJson { source })
}
