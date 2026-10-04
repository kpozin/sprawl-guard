use clap::{Args, Subcommand};

/// Hidden build-time CLI generation commands.
#[derive(Debug, Args)]
pub(crate) struct MachineArgs {
    /// Generation command to run.
    #[command(subcommand)]
    pub(super) command: MachineSubcommand,
}

/// Supported hidden generation commands.
#[derive(Debug, Subcommand)]
pub(crate) enum MachineSubcommand {
    /// Emit the structured CLI reference document.
    #[command(hide = true)]
    GenerateCliReferenceJson,
}
