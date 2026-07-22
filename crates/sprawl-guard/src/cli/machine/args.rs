use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::super::ColorModeArg;

/// Parsed top-level CLI arguments forwarded into `machine` subcommands.
pub(crate) struct MachineCliArgs {
    pub(super) root: Option<PathBuf>,
    pub(super) config: Option<PathBuf>,
    pub(super) color: Option<ColorModeArg>,
    pub(super) quiet: bool,
    pub(super) command: MachineArgs,
}

impl MachineCliArgs {
    pub(crate) fn new(
        root: Option<PathBuf>,
        config: Option<PathBuf>,
        color: Option<ColorModeArg>,
        quiet: bool,
        command: MachineArgs,
    ) -> Self {
        Self {
            root,
            config,
            color,
            quiet,
            command,
        }
    }
}

/// Machine-oriented CLI commands.
#[derive(Debug, Args)]
pub(crate) struct MachineArgs {
    /// Machine subcommand to run.
    #[command(subcommand)]
    pub(super) command: MachineSubcommand,
}

/// Supported machine subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum MachineSubcommand {
    /// Execute one machine request read from stdin.
    Run,
    /// Emit the TypeScript machine contract module.
    #[command(hide = true)]
    GenerateContractTypescript,
    /// Emit the structured CLI reference document.
    #[command(hide = true)]
    GenerateCliReferenceJson,
}
