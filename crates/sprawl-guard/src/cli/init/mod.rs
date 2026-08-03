mod advisory;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use sprawl_guard_lib::classification::discover_sources;
use sprawl_guard_lib::config::{built_in_config, supported_language_ids};
use sprawl_guard_lib::{LanguageId, SprawlError};

use crate::error::{CliError, Result};

use super::InitArgs;
use advisory::{InitAdvisory, advisory_json, run_advisory_scan, write_advisory_summary};

const PRIMARY_CONFIG_FILE: &str = "sprawl-guard.toml";
const ALIAS_CONFIG_FILE: &str = ".sprawlrc.toml";

pub(super) enum InitExecutionMode {
    DryRun { overwrite_warning: Option<PathBuf> },
    Written,
}

pub(super) struct InitExecution {
    advisory: InitAdvisory,
    config_text: String,
    mode: InitExecutionMode,
    selected_languages: Vec<LanguageId>,
    target: PathBuf,
}

pub(super) fn execute_init(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: InitArgs,
) -> Result<InitExecution> {
    let root = resolve_cli_root(root)?;
    let target = resolve_target_config_path(&root, config_path.as_deref());
    reject_existing_config_for_write(&root, &target, config_path.as_deref(), args.dry_run)?;
    let selected_languages = select_languages(&root, &args)?;
    let config_text = render_minimal_config(&selected_languages)?;
    let selected_config = selected_language_config(selected_languages.iter().copied())?;
    let advisory = run_advisory_scan(&root, &target, &selected_config)?;
    let overwrite_warning = existing_config_for_write(&root, &target, config_path.as_deref());
    let mode = if args.dry_run {
        InitExecutionMode::DryRun { overwrite_warning }
    } else {
        fs::write(&target, &config_text).map_err(|source| CliError::WriteConfig {
            path: target.clone(),
            source,
        })?;
        InitExecutionMode::Written
    };

    Ok(InitExecution {
        advisory,
        config_text,
        mode,
        selected_languages,
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
            output.push('\n');
        }
        InitExecutionMode::Written => {
            output.push_str("Selected languages:\n");
            for language in &execution.selected_languages {
                writeln!(&mut output, "  {}", init_language_key(*language)).unwrap();
            }
            output.push('\n');
            writeln!(&mut output, "Wrote {}", execution.target.display()).unwrap();
            output.push('\n');
        }
    }
    write_advisory_summary(&mut output, &execution.advisory);
    output
}

pub(super) fn init_payload(execution: &InitExecution) -> serde_json::Value {
    json!({
        "kind": "init",
        "target": execution.target,
        "selected_languages": execution.selected_languages,
        "config": toml::from_str::<toml::Value>(&execution.config_text)
            .expect("generated init config must stay valid TOML"),
        "mode": match &execution.mode {
            InitExecutionMode::DryRun { overwrite_warning } => json!({
                "kind": "dry_run",
                "overwrite_warning": overwrite_warning,
            }),
            InitExecutionMode::Written => json!({
                "kind": "written",
            }),
        },
        "advisory": advisory_json(&execution.advisory),
    })
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

fn reject_existing_config_for_write(
    root: &Path,
    target: &Path,
    explicit_config_path: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    if let Some(path) = existing_config_for_write(root, target, explicit_config_path) {
        return Err(CliError::ExistingConfig { path });
    }
    Ok(())
}

fn existing_config_for_write(
    root: &Path,
    target: &Path,
    explicit_config_path: Option<&Path>,
) -> Option<PathBuf> {
    if explicit_config_path.is_some() {
        return target.exists().then(|| target.to_path_buf());
    }
    [root.join(PRIMARY_CONFIG_FILE), root.join(ALIAS_CONFIG_FILE)]
        .into_iter()
        .find(|path| path.exists())
}

fn select_languages(root: &Path, args: &InitArgs) -> Result<Vec<LanguageId>> {
    if args.all_languages {
        return Ok(supported_language_ids());
    }
    if !args.languages.is_empty() {
        return Ok(sorted_unique_languages(args.languages.iter().copied()));
    }
    detect_languages(root)
}

fn detect_languages(root: &Path) -> Result<Vec<LanguageId>> {
    let config = built_in_config();
    let sources = discover_sources(root, &config)?;
    let languages = sorted_unique_languages(sources.into_iter().map(|source| source.language()));
    if languages.is_empty() {
        return Err(CliError::NoInitLanguages);
    }
    Ok(languages)
}

fn sorted_unique_languages(languages: impl IntoIterator<Item = LanguageId>) -> Vec<LanguageId> {
    languages
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn selected_language_config(
    languages: impl IntoIterator<Item = LanguageId>,
) -> Result<sprawl_guard_lib::config::Config> {
    let mut config = built_in_config();
    let selected = languages.into_iter().collect::<BTreeSet<_>>();
    config.disable_languages(
        supported_language_ids()
            .into_iter()
            .filter(|language| !selected.contains(language)),
    );
    Ok(config)
}

fn render_minimal_config(languages: &[LanguageId]) -> Result<String> {
    let mut root = toml::map::Map::new();
    let mut language_tables = toml::map::Map::new();
    for language in languages {
        let mut language_table = toml::map::Map::new();
        language_table.insert("enabled".to_owned(), toml::Value::Boolean(true));
        language_tables.insert(
            init_language_key(*language),
            toml::Value::Table(language_table),
        );
    }
    root.insert("languages".to_owned(), toml::Value::Table(language_tables));
    let toml = toml::to_string_pretty(&toml::Value::Table(root))
        .map_err(|source| CliError::RenderConfig { source })?;
    Ok(format!("{header}{toml}", header = minimal_config_header()))
}

fn init_language_key(language: LanguageId) -> String {
    language.to_string().to_ascii_lowercase()
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

#[cfg(test)]
mod tests {
    use super::*;

    mod init_language_key {
        use super::*;

        #[test]
        fn it_lowercases_the_canonical_tokei_name() {
            assert_eq!(
                super::init_language_key(LanguageId::new("typescript").unwrap()),
                "typescript"
            );
            assert_eq!(
                super::init_language_key(LanguageId::new("c++").unwrap()),
                "c++"
            );
        }
    }
}
