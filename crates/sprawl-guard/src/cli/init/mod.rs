use std::fmt::Write as _;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use sprawl_guard_lib::SprawlError;
use sprawl_guard_lib::config::ConfigSchemaVersion;
use sprawl_guard_lib::file_io::create_text_new;

use crate::error::{CliError, Result};

use super::InitArgs;

const PRIMARY_CONFIG_FILE: &str = "sprawl-guard.toml";

/// Outcome of `init`.
pub(super) enum InitExecutionMode {
    /// The configuration was only previewed. `overwrite_warning` names an existing file that
    /// would make `--write` fail.
    Previewed { overwrite_warning: Option<PathBuf> },
    /// The configuration file was created.
    Written,
}

pub(super) struct InitExecution {
    config_text: String,
    mode: InitExecutionMode,
    /// The configuration path as human output shows it.
    shown_target: String,
}

pub(super) fn execute_init(default_root: PathBuf, args: InitArgs) -> Result<InitExecution> {
    let root = resolve_cli_root(args.project.root.unwrap_or(default_root))?;
    let target = resolve_target_config_path(&root, args.project.config.as_deref());
    let existing = existing_config(&target);
    let config_text = render_minimal_config()?;
    let mode = if !args.write {
        InitExecutionMode::Previewed {
            overwrite_warning: existing,
        }
    } else if let Some(path) = existing {
        return Err(CliError::ExistingConfig { path });
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
        shown_target: super::display_path(&root, &target),
    })
}

pub(super) fn render_init_stdout(execution: &InitExecution, quiet: bool) -> String {
    if quiet {
        return String::new();
    }

    let mut output = String::new();
    match &execution.mode {
        InitExecutionMode::Previewed { overwrite_warning } => {
            writeln!(&mut output, "Would write {}", execution.shown_target).unwrap();
            if overwrite_warning.is_some() {
                writeln!(
                    &mut output,
                    "--write would be refused because {} already exists.",
                    execution.shown_target
                )
                .unwrap();
            }
            output.push('\n');
            write!(&mut output, "{}", execution.config_text).unwrap();
            output.push_str("\nPreview only; use --write to create it.\n");
        }
        InitExecutionMode::Written => {
            writeln!(&mut output, "Wrote {}", execution.shown_target).unwrap();
        }
    }
    output
}

fn resolve_cli_root(root: PathBuf) -> Result<PathBuf> {
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

fn existing_config(target: &Path) -> Option<PathBuf> {
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
