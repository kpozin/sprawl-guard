use super::args::{MachineArgs, MachineSubcommand};
use crate::cli::execution::write_stdout;
use crate::cli::reference::render_cli_reference_json;
use crate::error::Result;

/// Runs one hidden build-time generation command.
pub(crate) fn run_machine(args: MachineArgs) -> Result<i32> {
    match args.command {
        MachineSubcommand::GenerateCliReferenceJson => {
            write_stdout(&render_cli_reference_json()?)?;
            Ok(0)
        }
    }
}
