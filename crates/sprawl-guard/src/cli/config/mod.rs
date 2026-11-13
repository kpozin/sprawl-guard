mod migrate;

use serde::Serialize;
use sprawl_guard_lib::config::{Config, LoadOptions, load_resolved_config};

use super::ConfigUpgradeArgs;
use crate::error::{CliError, Result};

pub(super) use migrate::{execute_config_migrate, render_config_migration_stdout};

/// Loaded config materialized for a config-inspection command.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ConfigCommandExecution {
    config: Box<Config>,
    rendering: ConfigRendering,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigRendering {
    Resolved,
    Authored,
}

/// Loads the fully resolved config for `sprawl-guard config resolved`.
pub(super) fn execute_config_resolved(options: &LoadOptions) -> Result<ConfigCommandExecution> {
    execute_config_command(options, ConfigRendering::Resolved)
}

/// Loads the fully explicit config for `sprawl-guard config export`.
pub(super) fn execute_config_export(options: &LoadOptions) -> Result<ConfigCommandExecution> {
    execute_config_command(options, ConfigRendering::Authored)
}

/// Renders the config execution result as canonical TOML.
pub(super) fn render_config_stdout(execution: &ConfigCommandExecution) -> Result<String> {
    match execution.rendering {
        ConfigRendering::Resolved => toml::to_string_pretty(&execution.config)
            .map_err(|source| CliError::RenderConfig { source }),
        ConfigRendering::Authored => render_authored_toml(&execution.config),
    }
}

/// Renders serializable configuration as authored TOML with the current editor directive.
pub(super) fn render_authored_toml(document: &impl Serialize) -> Result<String> {
    let toml =
        toml::to_string_pretty(document).map_err(|source| CliError::RenderConfig { source })?;
    Ok(format!(
        "{}\n{toml}",
        sprawl_guard_lib::config::ConfigSchemaVersion::CURRENT.schema_directive()
    ))
}

fn execute_config_command(
    options: &LoadOptions,
    rendering: ConfigRendering,
) -> Result<ConfigCommandExecution> {
    let loaded = load_resolved_config(options)?;
    Ok(ConfigCommandExecution {
        config: Box::new(loaded.config.materialized_for_inspection()),
        rendering,
    })
}
