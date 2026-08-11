use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::Value;
use sprawl_guard_lib::LanguageId;
use sprawl_guard_lib::classification::discover_sources;
use sprawl_guard_lib::config::{
    Config, LoadOptions, authored_config_json_schema, load_resolved_config,
};

use super::ConfigResolvedArgs;
use crate::error::{CliError, Result};

/// Loaded config materialized for a config-inspection command.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ConfigCommandExecution {
    document: ConfigCommandDocument,
}

impl ConfigCommandExecution {
    fn new(document: ConfigCommandDocument) -> Self {
        Self { document }
    }
}

/// A complete config or a view limited to detected languages.
#[derive(Clone, Debug, PartialEq)]
enum ConfigCommandDocument {
    Full(Box<Config>),
    Detected(toml::Value),
}

/// Loads the fully resolved config for `sprawl-guard config resolved`.
pub(super) fn execute_config_resolved(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: ConfigResolvedArgs,
) -> Result<ConfigCommandExecution> {
    execute_config_command(root, config_path, ConfigLanguageSelection::from(args))
}

/// Loads the fully explicit config for `sprawl-guard config export`.
///
/// Today this uses the same resolved config model as `config resolved`. Keeping the command path
/// separate now lets the two commands diverge later without changing wrapper integration points.
pub(super) fn execute_config_export(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> Result<ConfigCommandExecution> {
    execute_config_command(root, config_path, ConfigLanguageSelection::All)
}

/// Renders the config execution result as canonical TOML.
pub(super) fn render_config_stdout(execution: &ConfigCommandExecution) -> Result<String> {
    match &execution.document {
        ConfigCommandDocument::Full(config) => toml::to_string_pretty(config),
        ConfigCommandDocument::Detected(document) => toml::to_string_pretty(document),
    }
    .map_err(|source| CliError::RenderConfig { source })
}

/// Returns the structured config payload for machine-mode consumers.
pub(super) fn config_payload(execution: &ConfigCommandExecution) -> Result<Value> {
    match &execution.document {
        ConfigCommandDocument::Full(config) => serde_json::to_value(config),
        ConfigCommandDocument::Detected(document) => serde_json::to_value(document),
    }
    .map_err(|source| CliError::RenderJson { source })
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
    selection: ConfigLanguageSelection,
) -> Result<ConfigCommandExecution> {
    let root = root.unwrap_or(
        std::env::current_dir().map_err(|source| CliError::CurrentDirectory { source })?,
    );
    let loaded = load_resolved_config(&LoadOptions {
        root: root.clone(),
        config: config_path,
    })?;
    let config = loaded.config.materialized_for_inspection();
    let document = match selection {
        ConfigLanguageSelection::All => ConfigCommandDocument::Full(Box::new(config)),
        ConfigLanguageSelection::Detected => {
            let detected = discover_sources(&root, &loaded.config)?
                .into_iter()
                .map(|source| source.language())
                .collect();
            let mut document = toml::Value::try_from(config)
                .map_err(|source| CliError::RenderConfig { source })?;
            retain_detected_languages(&mut document, &detected);
            ConfigCommandDocument::Detected(document)
        }
    };
    Ok(ConfigCommandExecution::new(document))
}

enum ConfigLanguageSelection {
    All,
    Detected,
}

impl From<ConfigResolvedArgs> for ConfigLanguageSelection {
    fn from(args: ConfigResolvedArgs) -> Self {
        if args.detected {
            Self::Detected
        } else {
            Self::All
        }
    }
}

fn retain_detected_languages(document: &mut toml::Value, detected: &BTreeSet<LanguageId>) {
    document["languages"]
        .as_table_mut()
        .expect("serialized config languages must be a table")
        .retain(|key, _| {
            let language = LanguageId::new(key)
                .expect("serialized config language keys must remain supported");
            detected.contains(&language)
        });
}
