use serde::{Deserialize, Serialize};

use crate::GlobPattern;

use super::PathOverride;

/// Root-level include settings.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigInclude {
    /// Glob patterns for root-level override fragments.
    pub include: Vec<GlobPattern>,
}

pub(super) fn merge_config_include(config: &mut ConfigInclude, partial: PartialConfigInclude) {
    if let Some(value) = partial.include {
        config.include = value;
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialConfigInclude {
    include: Option<Vec<GlobPattern>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IncludeFragment {
    #[serde(default)]
    pub(super) overrides: Vec<PathOverride>,
}
