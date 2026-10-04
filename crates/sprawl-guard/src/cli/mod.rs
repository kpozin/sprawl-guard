mod check_output;
mod config;
mod execution;
mod explain_output;
mod init;
mod languages;
mod machine;
mod ratchet;
mod reference;
mod schema;

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
    /// Add ratchet ceilings for newly discovered violations.
    Update(UpdateArgs),
    /// Lower or remove existing ratchet ceilings after violations improve.
    Tighten(TightenArgs),
    /// Inspect configuration.
    Config(ConfigCommand),
    /// Print a JSON Schema for a Sprawl Guard TOML document.
    Schema(SchemaArgs),
    /// Explain how one path is classified.
    Explain(ExplainArgs),
    /// Hidden build-time generators.
    #[command(hide = true)]
    Machine(machine::MachineArgs),
}

/// Arguments for `init`.
#[derive(Debug, Args)]
struct InitArgs {
    /// Print the planned configuration without creating files.
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

/// Arguments for `update`.
#[derive(Debug, Args)]
struct UpdateArgs {
    /// Paths scanned for current violations.
    paths: Vec<PathBuf>,

    /// Write the ratchet changes instead of only previewing them.
    #[arg(long)]
    write: bool,

    /// Permit existing ratchet ceilings to increase to current measurements.
    #[arg(long)]
    allow_increases: bool,

    /// Show changed ceilings or the complete resulting TOML document.
    #[arg(long, default_value = "human")]
    format: RatchetFormat,
}

impl UpdateArgs {
    fn ceiling_update_policy(&self) -> ratchet::CeilingUpdatePolicy {
        if self.allow_increases {
            ratchet::CeilingUpdatePolicy::AllowIncreases
        } else {
            ratchet::CeilingUpdatePolicy::Preserve
        }
    }
}

/// Arguments for `tighten`.
#[derive(Debug, Args)]
struct TightenArgs {
    /// Paths whose ratchet entries are evaluated.
    paths: Vec<PathBuf>,

    /// Write the ratchet changes instead of only previewing them.
    #[arg(long)]
    write: bool,

    /// Show changed ceilings or the complete resulting TOML document.
    #[arg(long, default_value = "human")]
    format: RatchetFormat,
}

/// Output choices for ratchet previews and writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum RatchetFormat {
    /// Describe only added, raised, lowered, or removed ceilings.
    Human,
    /// Print the complete resulting ratchet as parseable TOML.
    Toml,
}

/// Human and JSON output formats shared by diagnostic commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum HumanJsonFormat {
    /// Human-readable text output.
    Human,
    /// JSON output.
    Json,
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
    /// Upgrade an authored config to the current schema.
    Upgrade(ConfigUpgradeArgs),
}

/// Arguments for `config upgrade`.
#[derive(Debug, Args)]
struct ConfigUpgradeArgs {
    /// Report whether an upgrade is needed without changing the file.
    #[arg(long)]
    check: bool,
}

/// Arguments for `schema`.
#[derive(Debug, Args)]
struct SchemaArgs {
    /// TOML document kind whose JSON Schema to print.
    #[arg(value_enum, value_name = "DOCUMENT")]
    format: SchemaDocumentKind,
}

/// Sprawl Guard TOML document kinds with published schemas.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SchemaDocumentKind {
    /// Root configuration files.
    Config,
    /// Ratchet files.
    Ratchet,
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
        quiet,
        command,
    } = cli;
    match command {
        Command::Machine(args) => machine::run_machine(args),
        command => {
            let default_root =
                std::env::current_dir().map_err(|source| CliError::CurrentDirectory { source })?;
            let outcome = execution::execute_cli_command(
                Cli {
                    root,
                    config,
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
