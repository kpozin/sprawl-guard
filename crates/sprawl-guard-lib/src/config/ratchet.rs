use serde::{Deserialize, Serialize};

use crate::RatchetFilePath;

/// Ratchet file settings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RatchetConfig {
    /// Repository-relative ratchet file path.
    pub file: RatchetFilePath,
    /// Whether stale ratchet entries fail checks.
    pub require_current: bool,
}

pub(super) fn merge_ratchet(config: &mut RatchetConfig, partial: PartialRatchetConfig) {
    if let Some(value) = partial.file {
        config.file = value;
    }
    if let Some(value) = partial.require_current {
        config.require_current = value;
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialRatchetConfig {
    file: Option<RatchetFilePath>,
    require_current: Option<bool>,
}
