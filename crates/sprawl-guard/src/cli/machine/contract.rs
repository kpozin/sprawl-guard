use serde::{Deserialize, Serialize};

/// Current machine transport protocol version accepted by `machine run`.
pub(super) const MACHINE_PROTOCOL_VERSION: u8 = 1;

/// Highest process-style exit code representable in a machine response.
pub(super) const MACHINE_MAX_PROCESS_EXIT_CODE: u8 = u8::MAX;

/// Exit code used when the machine transport reports an operational error envelope.
pub(super) const MACHINE_OPERATIONAL_ERROR_EXIT_CODE: u8 = 2;

/// Host path syntax used by a machine request path mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum MachineHostPathStyle {
    Posix,
    Windows,
}

impl MachineHostPathStyle {
    /// All currently accepted path syntaxes for host-to-guest path mapping.
    pub(super) const VARIANTS: [Self; 2] = [Self::Posix, Self::Windows];
}

/// Machine-readable runtime failure categories emitted by the transport envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MachineRuntimeErrorCode {
    AllWithPaths,
    AmbiguousRatchetReplacement,
    ExistingConfig,
    InvalidExitCode,
    LibraryError,
    MissingRatchetFile,
    NoInitLanguages,
    NotImplemented,
    RatchetRaises,
    RenderConfigFailed,
    RenderJsonFailed,
    WriteConfigFailed,
}

impl MachineRuntimeErrorCode {
    /// All runtime failure categories currently emitted by the machine transport.
    pub(super) const VARIANTS: [Self; 12] = [
        Self::AllWithPaths,
        Self::AmbiguousRatchetReplacement,
        Self::ExistingConfig,
        Self::InvalidExitCode,
        Self::LibraryError,
        Self::MissingRatchetFile,
        Self::NoInitLanguages,
        Self::NotImplemented,
        Self::RatchetRaises,
        Self::RenderConfigFailed,
        Self::RenderJsonFailed,
        Self::WriteConfigFailed,
    ];
}
