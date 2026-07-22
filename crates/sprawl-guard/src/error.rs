/// Error type for recoverable `sprawl-guard` CLI failures.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// A reusable library operation failed.
    #[error(transparent)]
    Library(#[from] sprawl_guard_lib::SprawlError),
    /// The current directory could not be resolved.
    #[error("failed to determine current directory: {source}")]
    CurrentDirectory {
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// The resolved config could not be rendered.
    #[error("failed to render config: {source}")]
    RenderConfig {
        /// TOML serializer error.
        source: toml::ser::Error,
    },
    /// An init target would overwrite an existing config.
    #[error("config file already exists: {path}")]
    ExistingConfig {
        /// Existing config path.
        path: std::path::PathBuf,
    },
    /// Init could not select any language.
    #[error("no supported source files found; pass --languages or --all-languages")]
    NoInitLanguages,
    /// The initial config could not be written.
    #[error("failed to write config {path}: {source}")]
    WriteConfig {
        /// Target config path.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Init could not finish its advisory scan before writing the proposed config.
    #[error("failed to run init advisory scan before writing {path}: {source}")]
    InitAdvisoryScan {
        /// Proposed config path.
        path: std::path::PathBuf,
        /// Underlying CLI failure.
        source: Box<CliError>,
    },
    /// JSON output could not be rendered.
    #[error("failed to render JSON: {source}")]
    RenderJson {
        /// JSON serializer error.
        source: serde_json::Error,
    },
    /// The generated TypeScript machine contract could not be rendered.
    #[error("failed to render machine contract: {source}")]
    RenderMachineContract {
        /// Template rendering error.
        source: askama::Error,
    },
    /// The generated CLI reference could not be rendered.
    #[error("failed to render CLI reference: {source}")]
    RenderCliReference {
        /// JSON serializer error.
        source: serde_json::Error,
    },
    /// A Rust-owned machine contract value did not serialize as a string.
    #[error("machine contract value {name} did not serialize as a string")]
    MachineContractValueNonString {
        /// Machine contract value name.
        name: &'static str,
    },
    /// Machine response construction received an impossible process-style exit code.
    #[error("machine command produced invalid exit code {exit_code}; expected 0..=255")]
    InvalidMachineExitCode {
        /// Invalid exit code.
        exit_code: i32,
    },
    /// Machine commands must take root/config/state from the JSON request instead of global flags.
    #[error("machine commands do not accept top-level --root, --config, --color, or --quiet")]
    MachineGlobalFlagsUnsupported,
    /// Machine transport commands cannot be executed through the shared user-command executor.
    #[error("machine commands cannot be nested inside machine execution")]
    MachineNestedInvocation,
    /// The machine request could not be read from stdin.
    #[error("failed to read machine request from stdin: {source}")]
    ReadStdin {
        /// Underlying stdin I/O error.
        source: std::io::Error,
    },
    /// Baseline without a scope would replace an existing ratchet file.
    #[error(
        "ratchet file already exists; pass PATHS... to replace scoped entries or --all to replace every entry"
    )]
    AmbiguousRatchetReplacement,
    /// Baseline would raise at least one existing ratchet ceiling.
    #[error("refusing to raise {count} ratchet ceiling(s) without --allow-raises")]
    RatchetRaises {
        /// Number of ratchet entries that would increase.
        count: usize,
    },
    /// `ratchet` updates require an existing ratchet file.
    #[error("ratchet file does not exist: {path}")]
    MissingRatchetFile {
        /// Missing ratchet file path.
        path: std::path::PathBuf,
    },
    /// Mutually exclusive scope options were combined.
    #[error("--all cannot be combined with PATHS...")]
    AllWithPaths,
}

/// Result alias used by `sprawl-guard` CLI code.
pub type Result<T> = std::result::Result<T, CliError>;
