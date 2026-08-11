use std::io::Read;
use std::path::PathBuf;

use clap::Parser;
use clap::error::ErrorKind;
use serde::Serialize;

use super::args::{MachineCliArgs, MachineSubcommand};
use super::contract::MachineRuntimeErrorCode;
use super::request::MachineRunRequest;
use super::response::{MachineCompleted, MachineExitCode, MachinePayload, MachineResponse};
use super::typescript_contract::render_machine_contract_typescript;
use crate::cli::execution::{CommandPayload, execute_cli_command};
use crate::cli::reference::render_cli_reference_json;
use crate::cli::{CheckArgs, Cli, Command, ConfigSubcommand};
use crate::error::{CliError, Result};

/// Runs a machine-oriented CLI command.
pub(crate) fn run_machine(args: MachineCliArgs) -> Result<i32> {
    if args.root.is_some() || args.config.is_some() || args.color.is_some() || args.quiet {
        return Err(CliError::MachineGlobalFlagsUnsupported);
    }

    match args.command.command {
        MachineSubcommand::Run => run_machine_request(),
        MachineSubcommand::GenerateContractTypescript => {
            print!("{}", render_machine_contract_typescript()?);
            Ok(0)
        }
        MachineSubcommand::GenerateCliReferenceJson => {
            print!("{}", render_cli_reference_json()?);
            Ok(0)
        }
    }
}

fn run_machine_request() -> Result<i32> {
    let mut request = String::new();
    std::io::stdin()
        .read_to_string(&mut request)
        .map_err(|source| CliError::ReadStdin { source })?;

    let request = match serde_json::from_str::<MachineRunRequest>(&request) {
        Ok(request) => request,
        Err(source) => {
            print_response(&MachineResponse::request_error(format!(
                "invalid machine request: {source}"
            )))?;
            return Ok(0);
        }
    };

    print_response(&execute_request(request))?;
    Ok(0)
}

fn execute_request(request: MachineRunRequest) -> MachineResponse {
    match execute_request_inner(&request) {
        Ok(completed) => MachineResponse::completed(completed),
        Err(MachineRunError::Request(message)) => MachineResponse::request_error(message),
        Err(MachineRunError::Runtime(error)) => {
            MachineResponse::runtime_error(machine_runtime_error_code(&error), format!("{error}\n"))
        }
    }
}

fn execute_request_inner(
    request: &MachineRunRequest,
) -> std::result::Result<MachineCompleted, MachineRunError> {
    let mut cli = match parse_machine_argv(request)? {
        MachineParseOutcome::Parsed(cli) => cli,
        MachineParseOutcome::Display {
            exit_code,
            stdout,
            stderr,
        } => {
            return Ok(MachineCompleted::new(
                exit_code,
                stdout,
                stderr,
                MachinePayload::Empty,
            ));
        }
    };
    ensure_machine_command_allowed(&cli)?;
    remap_cli_paths(request, &mut cli)?;

    let outcome = execute_cli_command(cli, request.workspace_root().to_path_buf())
        .map_err(MachineRunError::Runtime)?;
    let exit_code = MachineExitCode::try_from(outcome.exit_code).map_err(|exit_code| {
        MachineRunError::Runtime(CliError::InvalidMachineExitCode { exit_code })
    })?;
    Ok(MachineCompleted::new(
        exit_code,
        outcome.stdout,
        outcome.stderr,
        MachinePayload::from(outcome.payload),
    ))
}

impl From<CommandPayload> for MachinePayload {
    fn from(value: CommandPayload) -> Self {
        Self::Json(value.into_value())
    }
}

enum MachineParseOutcome {
    Parsed(Cli),
    Display {
        exit_code: MachineExitCode,
        stdout: String,
        stderr: String,
    },
}

fn parse_machine_argv(
    request: &MachineRunRequest,
) -> std::result::Result<MachineParseOutcome, MachineRunError> {
    let argv = std::iter::once(crate::CLI_NAME.to_owned())
        .chain(request.argv().iter().cloned())
        .collect::<Vec<_>>();
    match Cli::try_parse_from(argv) {
        Ok(cli) => Ok(MachineParseOutcome::Parsed(cli)),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            let exit_code = MachineExitCode::try_from(error.exit_code()).map_err(|exit_code| {
                MachineRunError::Runtime(CliError::InvalidMachineExitCode { exit_code })
            })?;
            let rendered = error.render().to_string();
            let (stdout, stderr) = if error.use_stderr() {
                (String::new(), rendered)
            } else {
                (rendered, String::new())
            };
            Ok(MachineParseOutcome::Display {
                exit_code,
                stdout,
                stderr,
            })
        }
        Err(error) => Err(MachineRunError::Request(error.render().to_string())),
    }
}

fn remap_cli_paths(
    request: &MachineRunRequest,
    cli: &mut Cli,
) -> std::result::Result<(), MachineRunError> {
    let default_root = request.workspace_root().to_path_buf();
    let root = match &cli.root {
        Some(root) => request.remap_path(root, &default_root)?,
        None => default_root,
    };
    cli.root = Some(root.clone());

    if let Some(config) = &mut cli.config {
        *config = request.remap_path(config, &root)?;
    }

    match &mut cli.command {
        Command::Check(args) => remap_check_paths(request, &root, args)?,
        Command::Explain(args) => {
            args.path = request.remap_path(&args.path, &root)?;
        }
        Command::Baseline(args) => remap_paths(request, &root, &mut args.paths)?,
        Command::Ratchet(args) => remap_paths(request, &root, &mut args.paths)?,
        Command::Config(_) | Command::Init(_) | Command::Languages(_) | Command::Machine(_) => {}
    }

    Ok(())
}

fn remap_check_paths(
    request: &MachineRunRequest,
    root: &std::path::Path,
    args: &mut CheckArgs,
) -> std::result::Result<(), MachineRunError> {
    remap_paths(request, root, &mut args.paths)
}

fn remap_paths(
    request: &MachineRunRequest,
    root: &std::path::Path,
    paths: &mut [PathBuf],
) -> std::result::Result<(), MachineRunError> {
    for path in paths {
        *path = request.remap_path(path, root)?;
    }
    Ok(())
}

fn ensure_machine_command_allowed(cli: &Cli) -> std::result::Result<(), MachineRunError> {
    match machine_command_policy(&cli.command) {
        MachineCommandPolicy::Allowed => Ok(()),
        MachineCommandPolicy::Rejected(command) => Err(disallowed_machine_command(command)),
    }
}

enum MachineCommandPolicy {
    Allowed,
    Rejected(&'static str),
}

fn machine_command_policy(command: &Command) -> MachineCommandPolicy {
    match command {
        Command::Check(_)
        | Command::Explain(_)
        | Command::Init(_)
        | Command::Languages(_)
        | Command::Baseline(_)
        | Command::Ratchet(_)
        | Command::Config(crate::cli::ConfigCommand {
            command: ConfigSubcommand::Resolved,
        })
        | Command::Config(crate::cli::ConfigCommand {
            command: ConfigSubcommand::Export,
        })
        | Command::Config(crate::cli::ConfigCommand {
            command: ConfigSubcommand::Schema,
        }) => MachineCommandPolicy::Allowed,
        Command::Machine(_) => MachineCommandPolicy::Rejected("machine"),
    }
}

fn disallowed_machine_command(command: &'static str) -> MachineRunError {
    MachineRunError::Request(format!("machine run does not allow command: {command}"))
}

fn print_response(response: &impl Serialize) -> Result<()> {
    let output =
        serde_json::to_string_pretty(response).map_err(|source| CliError::RenderJson { source })?;
    println!("{output}");
    Ok(())
}

enum MachineRunError {
    Request(String),
    Runtime(CliError),
}

impl From<String> for MachineRunError {
    fn from(value: String) -> Self {
        Self::Request(value)
    }
}

fn machine_runtime_error_code(error: &CliError) -> MachineRuntimeErrorCode {
    match error {
        CliError::Library { .. } | CliError::CurrentDirectory { .. } => {
            MachineRuntimeErrorCode::LibraryError
        }
        CliError::RenderConfig { .. } => MachineRuntimeErrorCode::RenderConfigFailed,
        CliError::RenderMachineContract { .. } | CliError::RenderCliReference { .. } => {
            MachineRuntimeErrorCode::RenderJsonFailed
        }
        CliError::MachineContractValueNonString { .. } => MachineRuntimeErrorCode::RenderJsonFailed,
        CliError::ExistingConfig { .. } => MachineRuntimeErrorCode::ExistingConfig,
        CliError::InvalidMachineExitCode { .. } => MachineRuntimeErrorCode::InvalidExitCode,
        CliError::InitAdvisoryScan { .. } => MachineRuntimeErrorCode::LibraryError,
        CliError::WriteConfig { .. } => MachineRuntimeErrorCode::WriteConfigFailed,
        CliError::RenderJson { .. } => MachineRuntimeErrorCode::RenderJsonFailed,
        CliError::MachineGlobalFlagsUnsupported
        | CliError::MachineNestedInvocation
        | CliError::ReadStdin { .. } => MachineRuntimeErrorCode::NotImplemented,
        CliError::AmbiguousRatchetReplacement => {
            MachineRuntimeErrorCode::AmbiguousRatchetReplacement
        }
        CliError::RatchetRaises { .. } => MachineRuntimeErrorCode::RatchetRaises,
        CliError::MissingRatchetFile { .. } => MachineRuntimeErrorCode::MissingRatchetFile,
        CliError::AllWithPaths => MachineRuntimeErrorCode::AllWithPaths,
    }
}
