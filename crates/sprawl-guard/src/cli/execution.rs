use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sprawl_guard_lib::check::check as run_policy_check;
use sprawl_guard_lib::config::{LoadOptions, OutputFormat, load_resolved_config};
use sprawl_guard_lib::{CheckOptions, SprawlError};

use super::{
    CheckArgs, CheckFormat, Cli, Command, ConfigCommand, ConfigSubcommand, HumanJsonFormat,
    check_output, config as config_cli, explain_output, init, languages, ratchet,
};
use crate::error::{CliError, Result};

/// Machine-readable payload produced by a command execution.
///
/// Commands produce the JSON value that the machine wrapper should consume instead of scraping
/// stdout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cli) struct CommandPayload(Value);

impl CommandPayload {
    fn json(value: Value) -> Self {
        Self(value)
    }

    pub(super) fn into_value(self) -> Value {
        self.0
    }
}

pub(in crate::cli) struct CommandOutcome {
    pub(super) exit_code: i32,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) payload: CommandPayload,
}

impl CommandOutcome {
    fn new(exit_code: i32, stdout: String, stderr: String, payload: CommandPayload) -> Self {
        Self {
            exit_code,
            stdout,
            stderr,
            payload,
        }
    }
}

pub(in crate::cli) fn execute_cli_command(
    cli: Cli,
    default_root: PathBuf,
) -> Result<CommandOutcome> {
    let Cli {
        root,
        config,
        color,
        quiet,
        command,
    } = cli;
    let _ = color;
    let root = root.unwrap_or(default_root);
    match command {
        Command::Config(config_command) => execute_config(root, config, config_command),
        Command::Languages(args) => execute_languages(root, args),
        Command::Check(args) => execute_check(root, config, quiet, args),
        Command::Explain(args) => execute_explain(root, config, args),
        Command::Init(args) => {
            let execution = init::execute_init(Some(root), config, args)?;
            Ok(CommandOutcome::new(
                0,
                init::render_init_stdout(&execution, quiet),
                String::new(),
                CommandPayload::json(init::init_payload(&execution)),
            ))
        }
        Command::Baseline(args) => {
            let execution = ratchet::execute_baseline(Some(root), config, args)?;
            Ok(CommandOutcome::new(
                0,
                ratchet::render_ratchet_stdout(&execution, quiet)?,
                String::new(),
                CommandPayload::json(ratchet::ratchet_payload(&execution)),
            ))
        }
        Command::Ratchet(args) => {
            let execution = ratchet::execute_ratchet(Some(root), config, args)?;
            Ok(CommandOutcome::new(
                0,
                ratchet::render_ratchet_stdout(&execution, quiet)?,
                String::new(),
                CommandPayload::json(ratchet::ratchet_payload(&execution)),
            ))
        }
        Command::Machine(_) => Err(CliError::MachineNestedInvocation),
    }
}

pub(super) fn print_command_outcome(outcome: &CommandOutcome) {
    print!("{}", outcome.stdout);
    eprint!("{}", outcome.stderr);
}

fn execute_check(
    root: PathBuf,
    config_path: Option<PathBuf>,
    quiet: bool,
    args: CheckArgs,
) -> Result<CommandOutcome> {
    let loaded = load_resolved_config(&LoadOptions {
        root: root.clone(),
        config: config_path,
    })?;
    let format = match args.format {
        Some(CheckFormat::Human) => CheckFormat::Human,
        Some(CheckFormat::Json) => CheckFormat::Json,
        Some(CheckFormat::Sarif) => CheckFormat::Sarif,
        None => check_format_from_config(&loaded.config.output.format)?,
    };
    let require_ratchet_current = require_ratchet_current_arg(&args);
    let report = match run_policy_check(CheckOptions {
        root: root.clone(),
        config: &loaded.config,
        paths: args.paths,
        require_ratchet_current,
        traversal_error_policy: args.traversal_error_policy.map(Into::into),
    }) {
        Ok(report) => report,
        Err(SprawlError::TraversalFailed { root, incidents }) if format == CheckFormat::Human => {
            let payload = traversal_failure_payload(&root, &incidents);
            return Ok(CommandOutcome::new(
                2,
                String::new(),
                check_output::render_traversal_failure_human(&root, &incidents),
                CommandPayload::json(payload),
            ));
        }
        Err(SprawlError::TraversalFailed { root, incidents }) if format == CheckFormat::Json => {
            let payload = traversal_failure_payload(&root, &incidents);
            return Ok(CommandOutcome::new(
                2,
                check_output::render_traversal_failure_json(&root, &incidents)?,
                String::new(),
                CommandPayload::json(payload),
            ));
        }
        Err(source) => return Err(source.into()),
    };
    let payload = json!({
        "kind": "report",
        "report": report,
    });
    Ok(CommandOutcome::new(
        if report.has_violations() { 1 } else { 0 },
        check_output::render_check_report(&report, format, quiet)?,
        String::new(),
        CommandPayload::json(payload),
    ))
}

fn traversal_failure_payload(
    root: &Path,
    incidents: &[sprawl_guard_lib::TraversalIncident],
) -> Value {
    json!({
        "kind": "traversal_failure",
        "report": check_output::traversal_failure_json_value(root, incidents),
    })
}

fn require_ratchet_current_arg(args: &CheckArgs) -> Option<bool> {
    if args.require_ratchet_current {
        return Some(true);
    }
    if args.no_require_ratchet_current {
        return Some(false);
    }
    None
}

fn execute_config(
    root: PathBuf,
    config_path: Option<PathBuf>,
    command: ConfigCommand,
) -> Result<CommandOutcome> {
    let execution = match command.command {
        ConfigSubcommand::Resolved => config_cli::execute_config_resolved(Some(root), config_path)?,
        ConfigSubcommand::Export => config_cli::execute_config_export(Some(root), config_path)?,
    };
    Ok(CommandOutcome::new(
        0,
        config_cli::render_config_stdout(&execution)?,
        String::new(),
        CommandPayload::json(config_cli::config_payload(&execution)?),
    ))
}

fn execute_explain(
    root: PathBuf,
    config_path: Option<PathBuf>,
    args: super::ExplainArgs,
) -> Result<CommandOutcome> {
    let loaded = load_resolved_config(&LoadOptions {
        root: root.clone(),
        config: config_path,
    })?;
    let classification =
        sprawl_guard_lib::classification::explain_path(&root, &args.path, &loaded.config)?;
    let payload =
        serde_json::to_value(&classification).map_err(|source| CliError::RenderJson { source })?;
    let stdout = match args.format.unwrap_or(HumanJsonFormat::Human) {
        HumanJsonFormat::Human => format!("{}\n", explain_output::explain_message(&classification)),
        HumanJsonFormat::Json => serde_json::to_string_pretty(&classification)
            .map(|output| format!("{output}\n"))
            .map_err(|source| CliError::RenderJson { source })?,
    };
    Ok(CommandOutcome::new(
        0,
        stdout,
        String::new(),
        CommandPayload::json(payload),
    ))
}

fn execute_languages(root: PathBuf, args: super::LanguagesArgs) -> Result<CommandOutcome> {
    let report = if args.detected {
        languages::detected_languages_report(&root)?
    } else {
        languages::supported_languages_report()
    };
    let payload = languages::json_report_value(&report);
    let stdout = match args.format.unwrap_or(HumanJsonFormat::Human) {
        HumanJsonFormat::Human => languages::human_report(&report),
        HumanJsonFormat::Json => {
            let mut output = languages::json_report(&report)?;
            output.push('\n');
            output
        }
    };
    Ok(CommandOutcome::new(
        0,
        stdout,
        String::new(),
        CommandPayload::json(payload),
    ))
}

fn check_format_from_config(format: &OutputFormat) -> Result<CheckFormat> {
    match format {
        OutputFormat::Human => Ok(CheckFormat::Human),
        OutputFormat::Json => Ok(CheckFormat::Json),
        OutputFormat::Sarif => Ok(CheckFormat::Sarif),
    }
}
