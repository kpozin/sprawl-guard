use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sprawl_guard_lib::{LanguageId, RelativePath};

/// The languages covered by the imported `max-lines` rule.
pub(super) fn js_languages() -> [LanguageId; 4] {
    ["javascript", "jsx", "typescript", "tsx"]
        .map(|key| LanguageId::new(key).expect("built-in JavaScript language key"))
}

/// The unqualified rule name used by both linters.
pub(super) const MAX_LINES: &str = "max-lines";
/// Oxlint's plugin-qualified name for the same rule.
pub(super) const OXLINT_MAX_LINES: &str = "eslint/max-lines";

/// A linter whose `max-lines` rule can be imported.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum Linter {
    /// ESLint.
    Eslint,
    /// Oxlint.
    Oxlint,
}

impl Linter {
    fn entry_point(self) -> &'static str {
        match self {
            Self::Eslint => "node_modules/eslint/bin/eslint.js",
            Self::Oxlint => "node_modules/oxlint/bin/oxlint",
        }
    }
}

impl fmt::Display for Linter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Eslint => "ESLint",
            Self::Oxlint => "Oxlint",
        })
    }
}

/// Linter operations used by config and ratchet import.
pub(super) enum Invocation<'a> {
    /// Print the ESLint configuration for one sample file.
    EslintConfig(&'a RelativePath),
    /// Print the root Oxlint configuration.
    OxlintConfig,
    /// Collect ESLint diagnostics for a nonempty batch of source files.
    EslintReport {
        first: &'a RelativePath,
        remaining: &'a [RelativePath],
    },
}

impl Invocation<'_> {
    fn linter(&self) -> Linter {
        match self {
            Self::EslintConfig(_) | Self::EslintReport { .. } => Linter::Eslint,
            Self::OxlintConfig => Linter::Oxlint,
        }
    }

    /// Arguments after the linter's entry point; paths are relative to the project root.
    fn args(&self) -> Vec<Arg<'_>> {
        match self {
            Self::EslintConfig(sample) => {
                vec![Arg::Text("--print-config"), Arg::Text(sample.as_str())]
            }
            Self::OxlintConfig => vec![Arg::Text("--print-config")],
            Self::EslintReport { first, remaining } => {
                let mut args = vec![
                    Arg::Text("--format"),
                    Arg::Text("json"),
                    Arg::Text("--no-error-on-unmatched-pattern"),
                    // An uncached ESLint run can remove the cache path it uses.
                    Arg::Text("--cache-location"),
                    Arg::Temporary {
                        temporary: "eslintcache",
                    },
                    Arg::Text("--"),
                    Arg::Text(first.as_str()),
                ];
                args.extend(remaining.iter().map(|path| Arg::Text(path.as_str())));
                args
            }
        }
    }

    /// Whether an exit code means the linter finished its work.
    fn completed(&self, code: Option<i32>) -> bool {
        // ESLint exits with 1 when it reports problems, which is the report this import reads.
        code == Some(0) || matches!(self, Self::EslintReport { .. }) && code == Some(1)
    }
}

/// One command-line argument passed to a linter.
#[derive(Serialize)]
#[serde(untagged)]
enum Arg<'a> {
    /// Literal text.
    Text(&'a str),
    /// A file with this name in a temporary directory that exists only for the run.
    Temporary { temporary: &'static str },
}

/// What a finished linter process wrote, and its exit code unless a signal ended it.
struct Output {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Runs a local linter from the project root and decodes its JSON output.
pub(super) fn run<T: DeserializeOwned>(
    root: &Path,
    invocation: Invocation<'_>,
) -> Result<T, ImportError> {
    let linter = invocation.linter();
    let output = execute(root, linter, &invocation.args())?;
    if !invocation.completed(output.code) {
        let detail = if output.stderr.is_empty() {
            &output.stdout
        } else {
            &output.stderr
        };
        return Err(ImportError::Process {
            linter,
            exit: LinterExit(output.code),
            detail: String::from_utf8_lossy(detail).trim().to_owned(),
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|source| ImportError::Output { linter, source })
}

#[cfg(not(target_os = "wasi"))]
fn execute(root: &Path, linter: Linter, args: &[Arg<'_>]) -> Result<Output, ImportError> {
    let entry = root
        .ancestors()
        .map(|dir| dir.join(linter.entry_point()))
        .find(|path| path.is_file())
        .ok_or(ImportError::MissingPackage { linter })?;
    // Created on first use, so runs without temporary files never touch the temporary directory.
    let mut temporary = None;
    let mut command = std::process::Command::new("node");
    command.current_dir(root).arg(entry);
    for arg in args {
        match arg {
            Arg::Text(text) => command.arg(text),
            Arg::Temporary { temporary: name } => {
                let directory = match &mut temporary {
                    Some(directory) => directory,
                    slot => slot.insert(
                        tempfile::tempdir()
                            .map_err(|source| ImportError::Temporary { linter, source })?,
                    ),
                };
                command.arg(directory.path().join(name))
            }
        };
    }
    let output = command
        .output()
        .map_err(|source| ImportError::Launch { linter, source })?;
    Ok(Output {
        code: output.status.code(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

/// Runs the linter through the npm wrapper, because WASI modules cannot start processes.
#[cfg(target_os = "wasi")]
fn execute(root: &Path, linter: Linter, args: &[Arg<'_>]) -> Result<Output, ImportError> {
    let request = host::Request::Run {
        root,
        entry: linter.entry_point(),
        args,
    };
    match host::call(&request)? {
        host::RunResponse::Exited {
            code,
            stdout,
            stderr,
        } => Ok(Output {
            code,
            stdout: stdout.into_bytes(),
            stderr: stderr.into_bytes(),
        }),
        host::RunResponse::MissingEntry => Err(ImportError::MissingPackage { linter }),
        host::RunResponse::TemporaryFailed { message } => Err(ImportError::Temporary {
            linter,
            source: io::Error::other(message),
        }),
        host::RunResponse::LaunchFailed { message } => Err(ImportError::Launch {
            linter,
            source: io::Error::other(message),
        }),
    }
}

/// Converts a file path from linter output into one comparable with the project root.
pub(super) fn reported_path(path: PathBuf) -> Result<PathBuf, ImportError> {
    #[cfg(target_os = "wasi")]
    {
        host::call(&host::Request::GuestPath { path: &path })
    }
    // Match the canonical project root's Windows verbatim prefix.
    #[cfg(windows)]
    {
        path.canonicalize()
            .map_err(|source| ImportError::Read { path, source })
    }
    #[cfg(not(any(windows, target_os = "wasi")))]
    {
        Ok(path)
    }
}

/// The `sprawl_guard_host` functions that the npm wrapper provides to the WASI build.
#[cfg(target_os = "wasi")]
mod host {
    use std::path::Path;

    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};

    use super::{Arg, ImportError};

    #[link(wasm_import_module = "sprawl_guard_host")]
    unsafe extern "C" {
        /// Handles one JSON request and returns the byte length of its JSON response.
        fn send_request(pointer: *const u8, length: usize) -> usize;
        /// Copies the response to the last request into a buffer of the returned length.
        fn read_response(pointer: *mut u8);
    }

    /// A request to the wrapper, matching `LinterHostRequest` in `linter-host.ts`.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) enum Request<'a> {
        /// Run a linter entry point found under the root or one of its parents.
        Run {
            root: &'a Path,
            entry: &'static str,
            args: &'a [Arg<'a>],
        },
        /// Spell a host path from linter output as this module sees it.
        GuestPath { path: &'a Path },
    }

    /// The outcome of a run request.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub(super) enum RunResponse {
        Exited {
            code: Option<i32>,
            stdout: String,
            stderr: String,
        },
        MissingEntry,
        TemporaryFailed {
            message: String,
        },
        LaunchFailed {
            message: String,
        },
    }

    /// Sends one request and decodes its response.
    pub(super) fn call<T: DeserializeOwned>(request: &Request<'_>) -> Result<T, ImportError> {
        let request = serde_json::to_vec(request).map_err(ImportError::Host)?;
        // SAFETY: the wrapper reads exactly `request.len()` bytes from this live buffer.
        let length = unsafe { send_request(request.as_ptr(), request.len()) };
        let mut response = vec![0; length];
        // SAFETY: the wrapper writes exactly the `length` bytes it just reported.
        unsafe { read_response(response.as_mut_ptr()) };
        serde_json::from_slice(&response).map_err(ImportError::Host)
    }
}

/// How a linter process that did not finish its work ended.
#[derive(Debug)]
pub(crate) struct LinterExit(Option<i32>);

impl fmt::Display for LinterExit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(code) => write!(formatter, "exit code {code}"),
            None => formatter.write_str("terminated by a signal"),
        }
    }
}

/// Recoverable failures while reading a linter's `max-lines` rule or suppressions.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ImportError {
    /// The project's linter package could not be found.
    #[error("{linter} was not found in node_modules under the project root or its parents")]
    MissingPackage { linter: Linter },
    /// The temporary directory for the linter's files could not be created.
    #[error("could not create a temporary directory for {linter}: {source}")]
    Temporary { linter: Linter, source: io::Error },
    /// The Node.js subprocess could not start.
    #[error("could not run {linter} with Node.js: {source}")]
    Launch { linter: Linter, source: io::Error },
    /// The linter exited with an operational error.
    #[error("{linter} failed ({exit}): {detail}")]
    Process {
        linter: Linter,
        exit: LinterExit,
        detail: String,
    },
    /// The linter response did not match its expected JSON format.
    #[error("could not read {linter} JSON output: {source}")]
    Output {
        linter: Linter,
        source: serde_json::Error,
    },
    /// The resolved configuration has no active file line rule.
    #[error("{linter} configuration has no enabled max-lines rule")]
    NoActiveLimit { linter: Linter },
    /// The resolved rule used an unknown numeric severity.
    #[error("unsupported max-lines severity {value}, expected 0, 1, or 2")]
    InvalidSeverity { value: u8 },
    /// The imported ceiling cannot be represented by Sprawl Guard.
    #[error("cannot import the max-lines ceiling: {0}")]
    InvalidLimit(#[from] sprawl_guard_lib::LimitValueError),
    /// The npm wrapper's linter bridge sent or received malformed JSON.
    #[cfg(target_os = "wasi")]
    #[error("could not exchange linter data with the npm wrapper: {0}")]
    Host(#[source] serde_json::Error),
    /// ESLint could not parse a candidate source file.
    #[error(
        "ESLint could not parse {path}; fix its fatal diagnostics before importing suppressions"
    )]
    EslintFatal { path: PathBuf },
    /// A candidate source or suppression file could not be read.
    #[error("could not read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    /// The bulk suppression file did not match its expected format.
    #[error("could not parse suppressions file {path}: {source}")]
    Suppressions {
        path: PathBuf,
        source: serde_json::Error,
    },
}
