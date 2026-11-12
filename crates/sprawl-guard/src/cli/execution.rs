use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use sprawl_guard_lib::check::check as run_policy_check;
use sprawl_guard_lib::config::{LoadOptions, OutputFormat, load_resolved_config};
use sprawl_guard_lib::{CheckOptions, SprawlError};

use super::{
    CheckArgs, CheckFormat, Cli, Command, ConfigCommand, ConfigSubcommand, HumanJsonFormat,
    check_output, config as config_cli, explain_output, init, languages, ratchet, schema,
};
use crate::error::{CliError, Result};

const WASI_STDOUT_RETRY_DELAY: Duration = Duration::from_millis(1);

pub(in crate::cli) struct CommandOutcome {
    pub(super) exit_code: i32,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

impl CommandOutcome {
    fn new(exit_code: i32, stdout: String, stderr: String) -> Self {
        Self {
            exit_code,
            stdout,
            stderr,
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
        quiet,
        command,
    } = cli;
    let root = root.unwrap_or(default_root);
    match command {
        Command::Config(config_command) => execute_config(root, config, quiet, config_command),
        Command::Schema(args) => {
            let document = schema::schema_payload(args.format)?;
            Ok(CommandOutcome::new(
                0,
                schema::render_schema_stdout(&document)?,
                String::new(),
            ))
        }
        Command::Languages(args) => execute_languages(root, args),
        Command::Check(args) => execute_check(root, config, quiet, args),
        Command::Explain(args) => execute_explain(root, config, args),
        Command::Init(args) => {
            let execution = init::execute_init(Some(root), config, args)?;
            Ok(CommandOutcome::new(
                0,
                init::render_init_stdout(&execution, quiet),
                String::new(),
            ))
        }
        Command::Update(args) => {
            ratchet_command_outcome(ratchet::execute_update(Some(root), config, args), quiet)
        }
        Command::Tighten(args) => {
            ratchet_command_outcome(ratchet::execute_tighten(Some(root), config, args), quiet)
        }
        Command::Machine(_) => unreachable!("machine commands are handled before shared execution"),
    }
}

fn ratchet_command_outcome(execution: Result<String>, quiet: bool) -> Result<CommandOutcome> {
    match execution {
        Ok(stdout) => Ok(CommandOutcome::new(
            0,
            if quiet { String::new() } else { stdout },
            String::new(),
        )),
        Err(CliError::Library(SprawlError::TraversalFailed { root, incidents })) => {
            Ok(CommandOutcome::new(
                2,
                String::new(),
                check_output::render_traversal_failure_human(&root, &incidents),
            ))
        }
        Err(error) => Err(error),
    }
}

pub(super) fn print_command_outcome(outcome: &CommandOutcome) -> Result<()> {
    write_stdout(&outcome.stdout)?;
    eprint!("{}", outcome.stderr);
    Ok(())
}

/// Writes command output through WASI hosts that expose stdout as a nonblocking pipe.
pub(super) fn write_stdout(output: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let mut remaining = output.as_bytes();
    while !remaining.is_empty() {
        let written = retry_stdout(|| stdout.write(remaining)).map_err(CliError::WriteStdout)?;
        if written == 0 {
            return Err(CliError::WriteStdout(io::ErrorKind::WriteZero.into()));
        }
        remaining = &remaining[written..];
    }
    retry_stdout(|| stdout.flush()).map_err(CliError::WriteStdout)
}

fn retry_stdout<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match operation() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if cfg!(target_os = "wasi") && error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(WASI_STDOUT_RETRY_DELAY);
            }
            result => return result,
        }
    }
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
    let policy = loaded.resolved_policy();
    let config_source = policy.source().clone();
    let format = match args.format {
        Some(CheckFormat::Human) => CheckFormat::Human,
        Some(CheckFormat::Json) => CheckFormat::Json,
        Some(CheckFormat::Sarif) => CheckFormat::Sarif,
        None => check_format_from_config(&loaded.config.output.format)?,
    };
    let require_ratchet_current = require_ratchet_current_arg(&args);
    let report = match run_policy_check(CheckOptions {
        root: root.clone(),
        policy,
        paths: args.paths,
        require_ratchet_current,
        traversal_error_policy: args.traversal_error_policy.map(Into::into),
    }) {
        Ok(report) => report,
        Err(SprawlError::TraversalFailed { root, incidents }) if format == CheckFormat::Human => {
            return Ok(CommandOutcome::new(
                2,
                String::new(),
                check_output::render_check_traversal_failure_human(
                    &root,
                    &config_source,
                    &incidents,
                ),
            ));
        }
        Err(SprawlError::TraversalFailed { root, incidents }) if format == CheckFormat::Json => {
            return Ok(CommandOutcome::new(
                2,
                check_output::render_traversal_failure_json(&root, &config_source, &incidents)?,
                String::new(),
            ));
        }
        Err(source) => return Err(source.into()),
    };
    Ok(CommandOutcome::new(
        if report.has_violations() { 1 } else { 0 },
        check_output::render_check_report(&report, format, quiet)?,
        String::new(),
    ))
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
    quiet: bool,
    command: ConfigCommand,
) -> Result<CommandOutcome> {
    let execution = match command.command {
        ConfigSubcommand::Resolved => config_cli::execute_config_resolved(Some(root), config_path)?,
        ConfigSubcommand::Export => config_cli::execute_config_export(Some(root), config_path)?,
        ConfigSubcommand::Upgrade(args) => {
            let execution = config_cli::execute_config_migrate(root, config_path, args)?;
            return Ok(CommandOutcome::new(
                execution.exit_code(),
                config_cli::render_config_migration_stdout(&execution, quiet),
                String::new(),
            ));
        }
    };
    Ok(CommandOutcome::new(
        0,
        config_cli::render_config_stdout(&execution)?,
        String::new(),
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
    let stdout = match args.format.unwrap_or(HumanJsonFormat::Human) {
        HumanJsonFormat::Human => format!("{}\n", explain_output::explain_message(&classification)),
        HumanJsonFormat::Json => serde_json::to_string_pretty(&classification)
            .map(|output| format!("{output}\n"))
            .map_err(|source| CliError::RenderJson { source })?,
    };
    Ok(CommandOutcome::new(0, stdout, String::new()))
}

fn execute_languages(root: PathBuf, args: super::LanguagesArgs) -> Result<CommandOutcome> {
    let report = if args.detected {
        languages::detected_languages_report(&root)?
    } else {
        languages::supported_languages_report()
    };
    let stdout = match args.format.unwrap_or(HumanJsonFormat::Human) {
        HumanJsonFormat::Human => languages::human_report(&report),
        HumanJsonFormat::Json => {
            let mut output = languages::json_report(&report)?;
            output.push('\n');
            output
        }
    };
    Ok(CommandOutcome::new(0, stdout, String::new()))
}

fn check_format_from_config(format: &OutputFormat) -> Result<CheckFormat> {
    match format {
        OutputFormat::Human => Ok(CheckFormat::Human),
        OutputFormat::Json => Ok(CheckFormat::Json),
        OutputFormat::Sarif => Ok(CheckFormat::Sarif),
    }
}
