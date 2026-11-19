use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use sprawl_guard_lib::check::check as run_check;
use sprawl_guard_lib::config::{OutputFormat, load_resolved_config};
use sprawl_guard_lib::{CheckOptions, CheckStatus, SprawlError};

use super::{
    CheckArgs, CheckFormat, Command, ConfigCommand, ConfigSubcommand, HumanJsonFormat,
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
    command: Command,
    default_root: PathBuf,
) -> Result<CommandOutcome> {
    match command {
        Command::Config(config_command) => execute_config(default_root, config_command),
        Command::Schema(args) => Ok(CommandOutcome::new(
            0,
            schema::schema_stdout(args.format).to_owned(),
            String::new(),
        )),
        Command::Languages(args) => {
            let root = args.root.clone().unwrap_or(default_root);
            execute_languages(root, args)
        }
        Command::Check(args) => execute_check(default_root, args),
        Command::Explain(args) => execute_explain(default_root, args),
        Command::Init(args) => {
            let quiet = args.quiet.quiet;
            let execution = init::execute_init(default_root, args)?;
            Ok(CommandOutcome::new(
                0,
                init::render_init_stdout(&execution, quiet),
                String::new(),
            ))
        }
        Command::Update(args) => {
            let quiet = args.quiet.quiet;
            ratchet_command_outcome(ratchet::execute_update(default_root, args), quiet)
        }
        Command::Tighten(args) => {
            let quiet = args.quiet.quiet;
            ratchet_command_outcome(ratchet::execute_tighten(default_root, args), quiet)
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
        Err(CliError::Library(SprawlError::TraversalFailed {
            root,
            unreadable_paths,
        })) => Ok(CommandOutcome::new(
            2,
            String::new(),
            ratchet::render_refusal(&root, &unreadable_paths),
        )),
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

fn execute_check(default_root: PathBuf, args: CheckArgs) -> Result<CommandOutcome> {
    let quiet = args.quiet.quiet;
    let require_ratchet_current = require_ratchet_current_arg(&args);
    let load_options = args.project.load_options(default_root);
    let loaded = load_resolved_config(&load_options)?;
    let policy = loaded.resolved_config();
    let format = match args.format {
        Some(format) => format,
        None => check_format_from_config(&loaded.config.output.format),
    };
    let max_findings = args
        .max_findings
        .unwrap_or(loaded.config.output.max_findings);
    let report = run_check(CheckOptions {
        root: load_options.root.clone(),
        policy,
        paths: args.paths,
        require_ratchet_current,
        unreadable_paths: args.unreadable_paths.map(Into::into),
    })?;
    Ok(CommandOutcome::new(
        check_exit_code(report.status()),
        check_output::render_check_report(&report, format, quiet, max_findings)?,
        String::new(),
    ))
}

/// Maps a check result to the documented exit code.
const fn check_exit_code(status: CheckStatus) -> i32 {
    match status {
        CheckStatus::Passed => 0,
        CheckStatus::Failed => 1,
        CheckStatus::Error => 2,
    }
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

fn execute_config(default_root: PathBuf, command: ConfigCommand) -> Result<CommandOutcome> {
    let execution = match command.command {
        ConfigSubcommand::Resolved(project) => {
            config_cli::execute_config_resolved(&project.load_options(default_root))?
        }
        ConfigSubcommand::Export(project) => {
            config_cli::execute_config_export(&project.load_options(default_root))?
        }
        ConfigSubcommand::Import(args) => {
            let stdout = config_cli::execute_config_import(default_root, args)?;
            return Ok(CommandOutcome::new(0, stdout, String::new()));
        }
        ConfigSubcommand::Upgrade(args) => {
            let quiet = args.quiet.quiet;
            let execution = config_cli::execute_config_migrate(default_root, args)?;
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

fn execute_explain(default_root: PathBuf, args: super::ExplainArgs) -> Result<CommandOutcome> {
    let load_options = args.project.load_options(default_root);
    let loaded = load_resolved_config(&load_options)?;
    let classification = sprawl_guard_lib::classification::explain_path(
        &load_options.root,
        &args.path,
        &loaded.config,
    )?;
    let file_bytes = sprawl_guard_lib::classification::explain_file_bytes(
        &load_options.root,
        &classification,
        &loaded.config,
    )?;
    let explanation = explain_output::PathExplanation {
        classification,
        file_bytes,
    };
    let stdout = match args.format.unwrap_or(HumanJsonFormat::Human) {
        HumanJsonFormat::Human => format!("{}\n", explain_output::explain_message(&explanation)),
        HumanJsonFormat::Json => serde_json::to_string_pretty(&explanation)
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

fn check_format_from_config(format: &OutputFormat) -> CheckFormat {
    match format {
        OutputFormat::Human => CheckFormat::Human,
        OutputFormat::Json => CheckFormat::Json,
        OutputFormat::Sarif => CheckFormat::Sarif,
        OutputFormat::Github => CheckFormat::Github,
    }
}
