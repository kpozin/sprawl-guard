use std::path::PathBuf;

use serde::Serialize;
use sprawl_guard_lib::config::{
    AuthoredConfigPath, ConfigUpgrade, LoadOptions, prepare_config_upgrade,
};
use sprawl_guard_lib::file_io::replace_text_atomically;

use super::ConfigUpgradeArgs;
use crate::error::{CliError, Result};

const SUCCESS_EXIT_CODE: i32 = 0;
const MIGRATION_REQUIRED_EXIT_CODE: i32 = 1;

/// Result of inspecting or migrating an authored root configuration.
pub(in crate::cli) struct ConfigMigrationExecution {
    path: AuthoredConfigPath,
    status: ConfigMigrationStatus,
}

impl ConfigMigrationExecution {
    /// Returns the process exit code for this migration state.
    pub(in crate::cli) fn exit_code(&self) -> i32 {
        match self.status {
            ConfigMigrationStatus::Current | ConfigMigrationStatus::Migrated => SUCCESS_EXIT_CODE,
            ConfigMigrationStatus::Required => MIGRATION_REQUIRED_EXIT_CODE,
        }
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConfigMigrationStatus {
    Current,
    Required,
    Migrated,
}

enum ConfigMigrationMode {
    Check,
    Write,
}

impl From<ConfigUpgradeArgs> for ConfigMigrationMode {
    fn from(args: ConfigUpgradeArgs) -> Self {
        if args.check { Self::Check } else { Self::Write }
    }
}

/// Inspects the root config and upgrades it when requested.
pub(in crate::cli) fn execute_config_migrate(
    root: PathBuf,
    config_path: Option<PathBuf>,
    args: ConfigUpgradeArgs,
) -> Result<ConfigMigrationExecution> {
    let mode = ConfigMigrationMode::from(args);
    let prepared = prepare_config_upgrade(&LoadOptions {
        root,
        config: config_path,
    })?
    .ok_or(CliError::MissingConfigForMigration)?;
    let (path, upgrade) = prepared.into_parts();

    match upgrade {
        ConfigUpgrade::Current => Ok(ConfigMigrationExecution {
            path,
            status: ConfigMigrationStatus::Current,
        }),
        ConfigUpgrade::Required(_) if matches!(mode, ConfigMigrationMode::Check) => {
            Ok(ConfigMigrationExecution {
                path,
                status: ConfigMigrationStatus::Required,
            })
        }
        ConfigUpgrade::Required(config) => {
            replace_text_atomically(path.as_path(), &config.to_string()).map_err(|source| {
                CliError::MigrateConfigIo {
                    path: path.as_path().to_path_buf(),
                    source,
                }
            })?;
            Ok(ConfigMigrationExecution {
                path,
                status: ConfigMigrationStatus::Migrated,
            })
        }
    }
}

/// Renders a concise human result for `config upgrade`.
pub(in crate::cli) fn render_config_migration_stdout(
    execution: &ConfigMigrationExecution,
    quiet: bool,
) -> String {
    if quiet {
        return String::new();
    }
    let action = match execution.status {
        ConfigMigrationStatus::Current => "Config is current",
        ConfigMigrationStatus::Required => "Config requires an upgrade",
        ConfigMigrationStatus::Migrated => "Upgraded config",
    };
    format!("{action}: {}\n", execution.path.as_path().display())
}
