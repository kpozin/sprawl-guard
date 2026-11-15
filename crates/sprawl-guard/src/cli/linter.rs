use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::ValueEnum;
use serde::de::DeserializeOwned;
use sprawl_guard_lib::LanguageId;

/// The languages covered by the imported linter policy.
pub(super) fn js_languages() -> [LanguageId; 4] {
    ["javascript", "jsx", "typescript", "tsx"]
        .map(|key| LanguageId::new(key).expect("built-in JavaScript language key"))
}

/// The unqualified rule name used by both linters.
pub(super) const MAX_LINES: &str = "max-lines";
/// Oxlint's plugin-qualified name for the same rule.
pub(super) const OXLINT_MAX_LINES: &str = "eslint/max-lines";

/// A linter whose file-size policy can be imported.
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
    EslintConfig(&'a Path),
    /// Print the root Oxlint configuration.
    OxlintConfig,
    /// Collect ESLint diagnostics for a nonempty batch of source files.
    EslintReport {
        first: &'a Path,
        remaining: &'a [PathBuf],
    },
}

impl Invocation<'_> {
    fn linter(&self) -> Linter {
        match self {
            Self::EslintConfig(_) | Self::EslintReport { .. } => Linter::Eslint,
            Self::OxlintConfig => Linter::Oxlint,
        }
    }

    fn configure(&self, command: &mut Command) -> Result<Option<tempfile::TempDir>, ImportError> {
        match self {
            Self::EslintConfig(path) => {
                command.arg("--print-config").arg(path);
            }
            Self::OxlintConfig => {
                command.arg("--print-config");
            }
            Self::EslintReport { first, remaining } => {
                // An uncached ESLint run can remove the cache path it uses.
                let cache = tempfile::tempdir().map_err(ImportError::Cache)?;
                command
                    .args(["--format", "json", "--no-error-on-unmatched-pattern"])
                    .arg("--cache-location")
                    .arg(cache.path().join("eslintcache"))
                    .arg("--")
                    .arg(first)
                    .args(*remaining);
                return Ok(Some(cache));
            }
        }
        Ok(None)
    }
}

/// Runs a local linter from the project root and decodes its JSON output.
pub(super) fn run<T: DeserializeOwned>(
    root: &Path,
    invocation: Invocation<'_>,
) -> Result<T, ImportError> {
    let linter = invocation.linter();
    if cfg!(target_os = "wasi") {
        return Err(ImportError::NeedsNative { linter });
    }
    let entry = root
        .ancestors()
        .map(|dir| dir.join(linter.entry_point()))
        .find(|path| path.is_file())
        .ok_or(ImportError::MissingPackage { linter })?;
    let mut command = Command::new("node");
    command.current_dir(root).arg(entry);
    let _cache = invocation.configure(&mut command)?;
    let output = command
        .output()
        .map_err(|source| ImportError::Launch { linter, source })?;
    if !(output.status.success()
        || matches!(invocation, Invocation::EslintReport { .. }) && output.status.code() == Some(1))
    {
        let detail = if output.stderr.is_empty() {
            &output.stdout
        } else {
            &output.stderr
        };
        return Err(ImportError::Process {
            linter,
            status: output.status,
            detail: String::from_utf8_lossy(detail).trim().to_owned(),
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|source| ImportError::Output { linter, source })
}

/// Recoverable failures while reading a linter's policy or suppressions.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ImportError {
    /// The build cannot launch a linter subprocess.
    #[error("{linter} import requires a native Sprawl Guard build")]
    NeedsNative { linter: Linter },
    /// The project's linter package could not be found.
    #[error("{linter} was not found in node_modules under the project root or its parents")]
    MissingPackage { linter: Linter },
    /// The Node.js subprocess could not start.
    #[error("could not run {linter} with Node.js: {source}")]
    Launch { linter: Linter, source: io::Error },
    /// The linter exited with an operational error.
    #[error("{linter} failed ({status}): {detail}")]
    Process {
        linter: Linter,
        status: std::process::ExitStatus,
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
    /// A disposable cache path could not be created.
    #[error("could not isolate the ESLint cache: {0}")]
    Cache(#[source] io::Error),
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
