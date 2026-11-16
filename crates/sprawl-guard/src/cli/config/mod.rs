mod import;
mod migrate;

use serde::Serialize;
use sprawl_guard_lib::config::{
    Config, LoadOptions, load_resolved_config, render_effective_config_toml,
};

use super::ConfigUpgradeArgs;
use crate::error::{CliError, Result};

pub(super) use import::execute_config_import;
pub(super) use migrate::{execute_config_migrate, render_config_migration_stdout};

/// Loaded config for a config-inspection command.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ConfigCommandExecution {
    config: Box<Config>,
    rendering: ConfigRendering,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigRendering {
    /// Effective values that checks use, for reading rather than reloading.
    Resolved,
    /// Complete authored configuration that loads back to the same values.
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

/// Leading comment for `config resolved`, whose effective values, such as `code` in
/// `count_lines`, are not valid in a configuration file.
const RESOLVED_CONFIG_HEADER: &str = "# Effective settings, for reading only. \
    Run `sprawl-guard config export` for a file you can load.\n";

/// Renders the config execution result as canonical TOML.
pub(super) fn render_config_stdout(execution: &ConfigCommandExecution) -> Result<String> {
    match execution.rendering {
        ConfigRendering::Resolved => render_effective_config_toml(&execution.config)
            .map(|toml| format!("{RESOLVED_CONFIG_HEADER}{toml}"))
            .map_err(|source| CliError::RenderConfig { source }),
        ConfigRendering::Authored => {
            render_authored_toml(&execution.config.materialized_for_inspection())
        }
    }
}

/// Renders serializable configuration as authored TOML with the current editor directive.
pub(super) fn render_authored_toml(document: &impl Serialize) -> Result<String> {
    let toml =
        toml::to_string_pretty(document).map_err(|source| CliError::RenderConfig { source })?;
    Ok(format!(
        "{}\n{toml}",
        sprawl_guard_lib::config::ConfigSchemaVersion::current_schema_directive()
    ))
}

fn execute_config_command(
    options: &LoadOptions,
    rendering: ConfigRendering,
) -> Result<ConfigCommandExecution> {
    let loaded = load_resolved_config(options)?;
    Ok(ConfigCommandExecution {
        config: Box::new(loaded.config),
        rendering,
    })
}
