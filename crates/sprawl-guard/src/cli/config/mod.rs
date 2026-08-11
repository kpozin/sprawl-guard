use std::path::PathBuf;

use serde_json::Value;
use sprawl_guard_lib::config::{
    Config, LoadOptions, authored_config_json_schema, load_resolved_config,
};

use crate::error::{CliError, Result};

/// Loaded config materialized for a config-inspection command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConfigCommandExecution {
    config: Config,
}

impl ConfigCommandExecution {
    fn new(config: Config) -> Self {
        Self { config }
    }
}

/// Loads the fully resolved config for `sprawl-guard config resolved`.
pub(super) fn execute_config_resolved(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> Result<ConfigCommandExecution> {
    execute_config_command(root, config_path)
}

/// Loads the fully explicit config for `sprawl-guard config export`.
///
/// Today this uses the same resolved config model as `config resolved`. Keeping the command path
/// separate now lets the two commands diverge later without changing wrapper integration points.
pub(super) fn execute_config_export(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> Result<ConfigCommandExecution> {
    execute_config_command(root, config_path)
}

/// Renders the config execution result as canonical TOML.
pub(super) fn render_config_stdout(execution: &ConfigCommandExecution) -> Result<String> {
    toml::to_string_pretty(&execution.config).map_err(|source| CliError::RenderConfig { source })
}

/// Returns the structured config payload for machine-mode consumers.
pub(super) fn config_payload(execution: &ConfigCommandExecution) -> Result<Value> {
    serde_json::to_value(&execution.config).map_err(|source| CliError::RenderJson { source })
}

/// Generates the structured JSON Schema payload for authored config files.
pub(super) fn config_schema_payload() -> Result<Value> {
    serde_json::to_value(authored_config_json_schema())
        .map_err(|source| CliError::RenderJson { source })
}

/// Renders the generated schema as compact JSON for direct distribution.
pub(super) fn render_schema_stdout(payload: &Value) -> Result<String> {
    serde_json::to_string(payload)
        .map(|output| format!("{output}\n"))
        .map_err(|source| CliError::RenderJson { source })
}

fn execute_config_command(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> Result<ConfigCommandExecution> {
    let root = root.unwrap_or(
        std::env::current_dir().map_err(|source| CliError::CurrentDirectory { source })?,
    );
    let loaded = load_resolved_config(&LoadOptions {
        root,
        config: config_path,
    })?;
    Ok(ConfigCommandExecution::new(
        loaded.config.materialized_for_inspection(),
    ))
}
