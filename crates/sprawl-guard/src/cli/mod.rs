mod check_output;
mod config;
mod execution;
mod explain_output;
mod init;
mod languages;
pub(crate) mod linter;
mod machine;
mod ratchet;
mod reference;
mod schema;

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use sprawl_guard_lib::UnreadablePathPolicy;
use sprawl_guard_lib::config::LoadOptions;

use crate::error::{CliError, Result};

/// Top-level `sprawl-guard` command-line arguments.
#[derive(Debug, Parser)]
#[command(
    name = crate::CLI_NAME,
    version,
    about = "Check that source files and directories stay within size limits, and keep existing violations from growing."
)]
pub struct Cli {
    /// Command to run.
    #[command(subcommand)]
    command: Command,
}

/// Supported CLI commands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Preview or create a configuration file.
    Init(InitArgs),
    /// List supported or detected languages.
    Languages(LanguagesArgs),
    /// Check source files and directories against their limits and the ratchet file.
    Check(CheckArgs),
    /// Add ratchet entries for new violations.
    Update(UpdateArgs),
    /// Lower or remove ratchet entries after violations shrink.
    Tighten(TightenArgs),
    /// Show, upgrade, or import configuration.
    Config(ConfigCommand),
    /// Print a JSON Schema for a Sprawl Guard TOML document.
    Schema(SchemaArgs),
    /// Show how one file or directory is classified and which limits apply to it.
    Explain(ExplainArgs),
    /// Hidden build-time generators.
    #[command(hide = true)]
    Machine(machine::MachineArgs),
}

/// Options that locate the repository and its configuration file.
#[derive(Debug, Args)]
#[command(next_help_heading = "Repository options")]
struct ProjectArgs {
    /// Repository root. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,

    /// Configuration file. Relative paths are resolved under `--root`. Defaults to
    /// `sprawl-guard.toml` under the root.
    #[arg(long)]
    config: Option<PathBuf>,
}

impl ProjectArgs {
    /// Builds the config load options, falling back to `default_root` for the repository root.
    fn load_options(self, default_root: PathBuf) -> LoadOptions {
        LoadOptions {
            root: self.root.unwrap_or(default_root),
            config: self.config,
        }
    }
}

/// Option that suppresses non-essential output.
#[derive(Debug, Args)]
#[command(next_help_heading = "Repository options")]
struct QuietArgs {
    /// Suppress non-essential output.
    #[arg(long)]
    quiet: bool,
}

/// Arguments for `init`.
#[derive(Debug, Args)]
struct InitArgs {
    #[command(flatten)]
    project: ProjectArgs,

    #[command(flatten)]
    quiet: QuietArgs,

    /// Create the configuration file instead of only previewing it.
    #[arg(long)]
    write: bool,
}

/// Arguments for `check`.
#[derive(Debug, Args)]
struct CheckArgs {
    #[command(flatten)]
    project: ProjectArgs,

    #[command(flatten)]
    quiet: QuietArgs,

    /// Output format. Defaults to `[output].format` in the configuration, or `human`.
    #[arg(long)]
    format: Option<CheckFormat>,

    /// Fail when the ratchet file has stale entries. This is the default.
    #[arg(long, conflicts_with = "no_require_ratchet_current")]
    require_ratchet_current: bool,

    /// Allow stale ratchet entries, for exploratory local checks.
    #[arg(long, conflicts_with = "require_ratchet_current")]
    no_require_ratchet_current: bool,

    /// What to do with files or directories that cannot be read. Defaults to
    /// `[traversal].unreadable_paths` in the configuration, or `fail`.
    #[arg(long)]
    unreadable_paths: Option<UnreadablePathsArg>,

    /// Paths to check. Checks the whole repository when omitted.
    paths: Vec<PathBuf>,
}

/// Arguments for `languages`.
#[derive(Debug, Args)]
struct LanguagesArgs {
    /// Repository root scanned by `--detected`. Defaults to the current directory.
    #[arg(long, requires = "detected")]
    root: Option<PathBuf>,

    /// Output format. Defaults to `human`.
    #[arg(long)]
    format: Option<HumanJsonFormat>,

    /// List languages detected under the root instead of every supported language.
    #[arg(long)]
    detected: bool,
}

/// Arguments for `update`.
#[derive(Debug, Args)]
struct UpdateArgs {
    #[command(flatten)]
    project: ProjectArgs,

    #[command(flatten)]
    quiet: QuietArgs,

    /// Paths to scan for new violations. Scans the whole repository when omitted.
    paths: Vec<PathBuf>,

    /// Save the ratchet changes instead of only previewing them.
    #[arg(long)]
    write: bool,

    /// Raise existing ratchet ceilings to current measurements. Without this flag, `update` never
    /// raises them.
    #[arg(long)]
    allow_increases: bool,

    /// Add entries only for oversized files that LINTER exempts from `max-lines`.
    #[arg(long, value_enum, value_name = "LINTER")]
    import: Option<linter::Linter>,

    /// Output format.
    #[arg(long, default_value = "human")]
    format: UpdateFormat,
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
    #[command(flatten)]
    project: ProjectArgs,

    #[command(flatten)]
    quiet: QuietArgs,

    /// Paths whose ratchet entries are evaluated. Evaluates the whole ratchet file when omitted.
    paths: Vec<PathBuf>,

    /// Save the ratchet changes instead of only previewing them.
    #[arg(long)]
    write: bool,

    /// Output format.
    #[arg(long, default_value = "human")]
    format: TightenFormat,
}

/// Output choices for ratchet previews and writes, shared by `update` and `tighten`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RatchetFormat {
    /// Changed ceilings only.
    Human,
    /// The complete resulting ratchet file.
    Toml,
}

/// `update --format` values, described in terms of what `update` can change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum UpdateFormat {
    /// List the ratchet entries that would be added or raised.
    Human,
    /// Print the complete resulting ratchet file as TOML.
    Toml,
}

impl From<UpdateFormat> for RatchetFormat {
    fn from(value: UpdateFormat) -> Self {
        match value {
            UpdateFormat::Human => Self::Human,
            UpdateFormat::Toml => Self::Toml,
        }
    }
}

/// `tighten --format` values, described in terms of what `tighten` can change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum TightenFormat {
    /// List the ratchet entries that would be lowered or removed.
    Human,
    /// Print the complete resulting ratchet file as TOML.
    Toml,
}

impl From<TightenFormat> for RatchetFormat {
    fn from(value: TightenFormat) -> Self {
        match value {
            TightenFormat::Human => Self::Human,
            TightenFormat::Toml => Self::Toml,
        }
    }
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

/// CLI override for how unreadable paths are handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum UnreadablePathsArg {
    /// List the unreadable paths found while scanning, then fail.
    Fail,
    /// Report the unreadable paths found while scanning as warnings, and continue.
    Warn,
}

impl From<UnreadablePathsArg> for UnreadablePathPolicy {
    fn from(value: UnreadablePathsArg) -> Self {
        match value {
            UnreadablePathsArg::Fail => Self::Fail,
            UnreadablePathsArg::Warn => Self::Warn,
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
    /// Print the effective settings: the built-in defaults merged with the configuration file.
    Resolved(ProjectArgs),
    /// Print a complete configuration suitable for committing.
    Export(ProjectArgs),
    /// Preview or save the configuration file upgraded to the current schema.
    Upgrade(ConfigUpgradeArgs),
    /// Import a JavaScript and TypeScript line limit from an installed linter.
    Import(ConfigImportArgs),
}

/// Arguments for `config import`.
#[derive(Debug, Args)]
struct ConfigImportArgs {
    #[command(flatten)]
    project: ProjectArgs,

    /// Save the configuration instead of only previewing it. Fails if the configuration file
    /// already exists.
    #[arg(long)]
    write: bool,

    #[command(subcommand)]
    source: ConfigImportSource,
}

/// The configuration lookup each linter supports.
#[derive(Debug, Subcommand)]
enum ConfigImportSource {
    /// Use the ESLint configuration that applies to one representative source file.
    Eslint {
        /// Source file, relative to the root, whose ESLint configuration to read.
        #[arg(value_name = "FILE")]
        sample: PathBuf,
    },
    /// Use the root Oxlint configuration.
    Oxlint,
}

/// Arguments for `config upgrade`.
#[derive(Debug, Args)]
struct ConfigUpgradeArgs {
    #[command(flatten)]
    project: ProjectArgs,

    #[command(flatten)]
    quiet: QuietArgs,

    /// Save the upgraded configuration instead of only previewing it.
    #[arg(long, conflicts_with = "check")]
    write: bool,

    /// Save nothing, and exit with code 1 when an upgrade is pending.
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
    #[command(flatten)]
    project: ProjectArgs,

    /// Output format. Defaults to `human`.
    #[arg(long)]
    format: Option<HumanJsonFormat>,

    /// File or directory to explain. A file path does not need to exist.
    path: PathBuf,
}

/// Formats a path for human output: relative to the repository root when it is inside the root,
/// and `.` for the root itself.
///
/// The root may be given in a different form than the path, for example relative or through a
/// symlink, so the canonical root is tried too. Paths outside the root stay as given.
fn display_path(root: &Path, path: &Path) -> String {
    let canonical_root = root.canonicalize().ok();
    let relative = [Some(root), canonical_root.as_deref()]
        .into_iter()
        .flatten()
        .find_map(|root| path.strip_prefix(root).ok());
    match relative {
        Some(relative) => display_relative(&relative.to_string_lossy()),
        None => escape_controls(&path.to_string_lossy()),
    }
}

/// Formats a repository-relative path for human output, using `.` for the root itself.
fn display_relative(path: &str) -> String {
    if path.is_empty() {
        ".".to_owned()
    } else {
        escape_controls(path)
    }
}

/// Escapes control characters, so a path in human output stays on one line and cannot send
/// commands to the terminal.
fn escape_controls(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    escaped
}

/// Runs the CLI and returns the intended process exit code.
pub fn run(cli: Cli) -> Result<i32> {
    match cli.command {
        Command::Machine(args) => machine::run_machine(args),
        command => {
            let default_root =
                std::env::current_dir().map_err(|source| CliError::CurrentDirectory { source })?;
            let outcome = execution::execute_cli_command(command, default_root)?;
            execution::print_command_outcome(&outcome)?;
            Ok(outcome.exit_code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod when_displaying_a_path {
        use super::*;

        #[test]
        fn it_shows_paths_inside_the_root_relative_to_it() {
            assert_eq!(
                display_path(Path::new("/repo"), Path::new("/repo/src/lib.rs")),
                "src/lib.rs"
            );
        }

        #[test]
        fn it_shows_the_root_itself_as_a_dot() {
            assert_eq!(display_path(Path::new("/repo"), Path::new("/repo")), ".");
        }

        #[test]
        fn it_keeps_paths_outside_the_root_as_given() {
            assert_eq!(
                display_path(
                    Path::new("/repo"),
                    Path::new("/elsewhere/sprawl-guard.toml")
                ),
                "/elsewhere/sprawl-guard.toml"
            );
        }

        #[test]
        fn it_escapes_control_characters() {
            assert_eq!(display_relative("src\n\u{1b}[2J"), r"src\n\u{1b}[2J");
        }
    }
}
