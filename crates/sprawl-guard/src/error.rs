use std::path::PathBuf;

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
    /// A config upgrade was requested but no authored root config exists.
    #[error("no config file found to upgrade")]
    MissingConfigForMigration,
    /// An authored config could not be replaced atomically.
    #[error("failed to replace config {path}: {source}")]
    MigrateConfigIo {
        /// Authored config path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// The initial config could not be written.
    #[error("failed to write config {path}: {source}")]
    WriteConfig {
        /// Target config path.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// JSON output could not be rendered.
    #[error("failed to render JSON: {source}")]
    RenderJson {
        /// JSON serializer error.
        source: serde_json::Error,
    },
    /// The generated CLI reference could not be rendered.
    #[error("failed to render CLI reference: {source}")]
    RenderCliReference {
        /// JSON serializer error.
        source: serde_json::Error,
    },
    /// `tighten` requires an existing ratchet file.
    #[error("ratchet file does not exist: {path}")]
    MissingRatchetFile {
        /// Missing ratchet file path.
        path: std::path::PathBuf,
    },
}

/// Result alias used by `sprawl-guard` CLI code.
pub type Result<T> = std::result::Result<T, CliError>;
