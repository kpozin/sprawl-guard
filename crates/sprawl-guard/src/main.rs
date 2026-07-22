use clap::Parser;

mod cli;
mod error;

use crate::cli::{Cli, run};

/// Binary name used in generated CLI help and diagnostics.
pub(crate) const CLI_NAME: &str = "sprawl-guard";

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    }
}
