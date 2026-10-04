use std::fmt::Write as _;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use sprawl_guard_lib::SprawlError;
use sprawl_guard_lib::config::ConfigSchemaVersion;
use sprawl_guard_lib::file_io::create_text_new;

use crate::error::{CliError, Result};

use super::InitArgs;

const PRIMARY_CONFIG_FILE: &str = "sprawl-guard.toml";

pub(super) enum InitExecutionMode {
    DryRun { overwrite_warning: Option<PathBuf> },
    Written,
}

pub(super) struct InitExecution {
    config_text: String,
    mode: InitExecutionMode,
    target: PathBuf,
}

pub(super) fn execute_init(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: InitArgs,
) -> Result<InitExecution> {
    let root = resolve_cli_root(root)?;
    let target = resolve_target_config_path(&root, config_path.as_deref());
    reject_existing_config_for_write(&target, args.dry_run)?;
    let config_text = render_minimal_config()?;
    let overwrite_warning = existing_config_for_write(&target);
    let mode = if args.dry_run {
        InitExecutionMode::DryRun { overwrite_warning }
    } else {
        create_text_new(&target, &config_text).map_err(|source| {
            if source.kind() == ErrorKind::AlreadyExists {
                CliError::ExistingConfig {
                    path: target.clone(),
                }
            } else {
                CliError::WriteConfig {
                    path: target.clone(),
                    source,
                }
            }
        })?;
        InitExecutionMode::Written
    };

    Ok(InitExecution {
        config_text,
        mode,
        target,
    })
}

pub(super) fn render_init_stdout(execution: &InitExecution, quiet: bool) -> String {
    if quiet {
        return String::new();
    }

    let mut output = String::new();
    match &execution.mode {
        InitExecutionMode::DryRun { overwrite_warning } => {
            writeln!(&mut output, "Would write {}", execution.target.display()).unwrap();
            if let Some(path) = overwrite_warning {
                writeln!(
                    &mut output,
                    "A real write would be refused because {} already exists.",
                    path.display()
                )
                .unwrap();
            }
            output.push('\n');
            write!(&mut output, "{}", execution.config_text).unwrap();
        }
        InitExecutionMode::Written => {
            writeln!(&mut output, "Wrote {}", execution.target.display()).unwrap();
        }
    }
    output
}

pub(super) fn resolve_cli_root(root: Option<PathBuf>) -> Result<PathBuf> {
    let root = root.unwrap_or(
        std::env::current_dir().map_err(|source| CliError::CurrentDirectory { source })?,
    );
    root.canonicalize()
        .map_err(|source| SprawlError::ResolveRoot { path: root, source }.into())
}

fn resolve_target_config_path(root: &Path, config_path: Option<&Path>) -> PathBuf {
    match config_path {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => root.join(path),
        None => root.join(PRIMARY_CONFIG_FILE),
    }
}

fn reject_existing_config_for_write(target: &Path, dry_run: bool) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    if let Some(path) = existing_config_for_write(target) {
        return Err(CliError::ExistingConfig { path });
    }
    Ok(())
}

fn existing_config_for_write(target: &Path) -> Option<PathBuf> {
    target.exists().then(|| target.to_path_buf())
}

fn render_minimal_config() -> Result<String> {
    let root = toml::Table::from_iter([(
        "schema_version".to_owned(),
        toml::Value::Integer(ConfigSchemaVersion::CURRENT.into()),
    )]);
    let toml = toml::to_string_pretty(&root).map_err(|source| CliError::RenderConfig { source })?;
    Ok(format!(
        "{}\n{}{toml}",
        ConfigSchemaVersion::CURRENT.schema_directive(),
        minimal_config_header()
    ))
}

fn minimal_config_header() -> &'static str {
    "# sprawl-guard.toml\n\
     #\n\
     # View all resolved defaults:\n\
     #   sprawl-guard config resolved\n\
     #\n\
     # Debug why a path is included or excluded:\n\
     #   sprawl-guard explain path/to/file\n\n"
}
