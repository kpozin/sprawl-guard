mod check_output;
mod config;
mod execution;
mod explain_output;
mod init;
mod languages;
mod machine;
mod ratchet;
mod reference;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use sprawl_guard_lib::TraversalErrorPolicy;

use crate::error::{CliError, Result};

/// Top-level `sprawl-guard` command-line arguments.
#[derive(Debug, Parser)]
#[command(
    name = crate::CLI_NAME,
    version,
    about = "Source-tree structure guard for local navigability"
)]
pub struct Cli {
    /// Repository root. Defaults to the current directory.
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    /// Config file path. Relative paths are resolved under `--root`.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Color output mode.
    #[arg(long, global = true)]
    color: Option<ColorModeArg>,

    /// Suppress non-essential output.
    #[arg(long, global = true)]
    quiet: bool,

    /// Command to run.
    #[command(subcommand)]
    command: Command,
}

/// Supported CLI commands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Create an initial config file.
    Init(InitArgs),
    /// List supported or detected languages.
    Languages(LanguagesArgs),
    /// Run enforcement.
    Check(CheckArgs),
    /// Create ratchet entries for current violations.
    Baseline(BaselineArgs),
    /// Update an existing ratchet file downward.
    Ratchet(RatchetArgs),
    /// Inspect configuration.
    Config(ConfigCommand),
    /// Explain how one path is classified.
    Explain(ExplainArgs),
    /// Machine-oriented integration commands.
    Machine(machine::MachineArgs),
}

/// Arguments for `init`.
#[derive(Debug, Args)]
struct InitArgs {
    /// Print planned config and advisory output without creating files.
    #[arg(long)]
    dry_run: bool,
}

/// Arguments for `check`.
#[derive(Debug, Args)]
struct CheckArgs {
    /// Output format.
    #[arg(long)]
    format: Option<CheckFormat>,

    /// Fail if the ratchet file is stale.
    #[arg(long, conflicts_with = "no_require_ratchet_current")]
    require_ratchet_current: bool,

    /// Allow stale ratchet entries for exploratory local checks.
    #[arg(long, conflicts_with = "require_ratchet_current")]
    no_require_ratchet_current: bool,

    /// Override how recoverable descendant traversal errors affect this check.
    #[arg(long)]
    traversal_error_policy: Option<TraversalErrorPolicyArg>,

    /// Paths to check. Defaults to the repository root.
    paths: Vec<PathBuf>,
}

/// Arguments for `languages`.
#[derive(Debug, Args)]
struct LanguagesArgs {
    /// Output format.
    #[arg(long)]
    format: Option<HumanJsonFormat>,

    /// List languages detected under the root instead of every supported language.
    #[arg(long)]
    detected: bool,
}

/// Arguments for `baseline`.
#[derive(Debug, Args)]
struct BaselineArgs {
    /// Paths whose ratchet entries should be previewed or replaced.
    paths: Vec<PathBuf>,

    /// Write the ratchet changes instead of only previewing them.
    #[arg(long)]
    write: bool,

    /// Replace the whole ratchet file.
    #[arg(long)]
    all: bool,

    /// Permit existing ratchet ceilings to increase.
    #[arg(long)]
    allow_raises: bool,
}

/// Arguments for `ratchet`.
#[derive(Debug, Args)]
struct RatchetArgs {
    /// Paths whose ratchet entries should be evaluated.
    paths: Vec<PathBuf>,

    /// Write the ratchet changes instead of only previewing them.
    #[arg(long)]
    write: bool,
}

/// Human and JSON output formats shared by diagnostic commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum HumanJsonFormat {
    /// Human-readable text output.
    Human,
    /// JSON output.
    Json,
}

/// CLI color mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ColorModeArg {
    /// Detect whether color should be emitted.
    Auto,
    /// Always emit color.
    Always,
    /// Never emit color.
    Never,
}

/// Supported check output formats.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CheckFormat {
    /// Human-readable text output.
    Human,
    /// JSON output.
    Json,
    /// SARIF output.
    Sarif,
}

/// CLI traversal error policy override.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum TraversalErrorPolicyArg {
    /// Collect recoverable traversal incidents, then fail the run.
    Fail,
    /// Report recoverable traversal incidents as warnings and continue.
    Warn,
}

impl From<TraversalErrorPolicyArg> for TraversalErrorPolicy {
    fn from(value: TraversalErrorPolicyArg) -> Self {
        match value {
            TraversalErrorPolicyArg::Fail => Self::Fail,
            TraversalErrorPolicyArg::Warn => Self::Warn,
        }
    }
}

/// Config subcommands.
#[derive(Debug, Args)]
struct ConfigCommand {
    /// Config command to run.
    #[command(subcommand)]
    command: ConfigSubcommand,
}

/// Supported config subcommands.
#[derive(Debug, Subcommand)]
enum ConfigSubcommand {
    /// Print the fully resolved config used by checks.
    Resolved,
    /// Print a full explicit config suitable for committing.
    Export,
}

/// Arguments for `explain`.
#[derive(Debug, Args)]
struct ExplainArgs {
    /// Output format.
    #[arg(long)]
    format: Option<HumanJsonFormat>,

    /// Path to explain.
    path: PathBuf,
}

/// Runs the CLI and returns the intended process exit code.
pub fn run(cli: Cli) -> Result<i32> {
    let Cli {
        root,
        config,
        color,
        quiet,
        command,
    } = cli;
    match command {
        Command::Machine(args) => machine::run_machine(machine::MachineCliArgs::new(
            root, config, color, quiet, args,
        )),
        command => {
            let default_root =
                std::env::current_dir().map_err(|source| CliError::CurrentDirectory { source })?;
            let outcome = execution::execute_cli_command(
                Cli {
                    root,
                    config,
                    color,
                    quiet,
                    command,
                },
                default_root,
            )?;
            execution::print_command_outcome(&outcome);
            Ok(outcome.exit_code)
        }
    }
}
