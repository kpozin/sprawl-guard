use clap::Parser;

mod cli;
mod error;

use crate::cli::{Cli, run};

/// Binary name used in generated CLI help and diagnostics.
pub(crate) const CLI_NAME: &str = "sprawl-guard";

fn main() {
    // WASI starts every module in `/`. The npm wrapper mounts host paths unchanged and names the
    // real working directory, so relative and absolute paths mean what they do natively.
    #[cfg(target_os = "wasi")]
    if let Some(directory) = std::env::var_os("PWD")
        && let Err(err) = std::env::set_current_dir(&directory)
    {
        eprintln!(
            "could not enter {}: {err}",
            std::path::Path::new(&directory).display()
        );
        std::process::exit(2);
    }
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    }
}
