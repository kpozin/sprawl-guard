use std::path::PathBuf;

use sprawl_guard_lib::config::{
    ConfigUpgrade, MigratedAuthoredConfigDocument, prepare_config_upgrade,
};
use sprawl_guard_lib::file_io::replace_text_atomically;

use super::ConfigUpgradeArgs;
use crate::error::{CliError, Result};

const SUCCESS_EXIT_CODE: i32 = 0;
const MIGRATION_REQUIRED_EXIT_CODE: i32 = 1;

/// Result of inspecting or migrating an authored root configuration.
pub(in crate::cli) struct ConfigMigrationExecution {
    /// The configuration path as human output shows it.
    shown_path: String,
    status: ConfigMigrationStatus,
}

impl ConfigMigrationExecution {
    /// Returns the process exit code for this migration state.
    pub(in crate::cli) fn exit_code(&self) -> i32 {
        match self.status {
            ConfigMigrationStatus::Current
            | ConfigMigrationStatus::Previewed(_)
            | ConfigMigrationStatus::Migrated => SUCCESS_EXIT_CODE,
            ConfigMigrationStatus::Required => MIGRATION_REQUIRED_EXIT_CODE,
        }
    }
}

/// Outcome of `config upgrade`.
enum ConfigMigrationStatus {
    /// The configuration already uses the current schema.
    Current,
    /// An upgrade is pending and was only previewed.
    Previewed(MigratedAuthoredConfigDocument),
    /// An upgrade is pending and `--check` asked for a failing exit code.
    Required,
    /// The upgraded configuration was saved.
    Migrated,
}

/// What `config upgrade` does with a pending upgrade.
#[derive(Clone, Copy)]
enum ConfigMigrationMode {
    /// Print the upgraded configuration without saving it.
    Preview,
    /// Save nothing, and exit with code 1.
    Check,
    /// Save the upgraded configuration.
    Write,
}

impl ConfigMigrationMode {
    fn from_args(args: &ConfigUpgradeArgs) -> Self {
        match (args.write, args.check) {
            (true, _) => Self::Write,
            (false, true) => Self::Check,
            (false, false) => Self::Preview,
        }
    }
}

/// Inspects the root config and upgrades it when requested.
pub(in crate::cli) fn execute_config_migrate(
    default_root: PathBuf,
    args: ConfigUpgradeArgs,
) -> Result<ConfigMigrationExecution> {
    let mode = ConfigMigrationMode::from_args(&args);
    let options = args.project.load_options(default_root);
    let prepared = prepare_config_upgrade(&options)?.ok_or(CliError::MissingConfigForMigration)?;
    let (path, upgrade) = prepared.into_parts();

    let status = match upgrade {
        ConfigUpgrade::Current => ConfigMigrationStatus::Current,
        ConfigUpgrade::Required(config) => match mode {
            ConfigMigrationMode::Preview => ConfigMigrationStatus::Previewed(config),
            ConfigMigrationMode::Check => ConfigMigrationStatus::Required,
            ConfigMigrationMode::Write => {
                replace_text_atomically(path.as_path(), &config.to_string()).map_err(|source| {
                    CliError::MigrateConfigIo {
                        path: path.as_path().to_path_buf(),
                        source,
                    }
                })?;
                ConfigMigrationStatus::Migrated
            }
        },
    };
    Ok(ConfigMigrationExecution {
        shown_path: crate::cli::display_path(&options.root, path.as_path()),
        status,
    })
}

/// Renders a concise human result for `config upgrade`.
pub(in crate::cli) fn render_config_migration_stdout(
    execution: &ConfigMigrationExecution,
    quiet: bool,
) -> String {
    if quiet {
        return String::new();
    }
    let path = &execution.shown_path;
    match &execution.status {
        ConfigMigrationStatus::Current => format!("Configuration is current: {path}\n"),
        ConfigMigrationStatus::Required => format!("Configuration needs an upgrade: {path}\n"),
        ConfigMigrationStatus::Migrated => format!("Upgraded {path}\n"),
        ConfigMigrationStatus::Previewed(config) => {
            format!("Would upgrade {path}\n\n{config}\nPreview only; use --write to apply.\n")
        }
    }
}
