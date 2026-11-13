use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::GlobPattern;

/// Global file matching settings.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilesConfig {
    /// Glob patterns excluded before language classification: the built-in patterns followed by
    /// every authored addition.
    pub extend_exclude: Vec<GlobPattern>,
}

pub(super) fn merge_global_file_excludes(config: &mut FilesConfig, partial: PartialFilesConfig) {
    if let Some(value) = partial.extend_exclude {
        config.extend_exclude.extend(value);
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialFilesConfig {
    /// Glob patterns excluded before language classification, added to the built-in patterns.
    extend_exclude: Option<Vec<GlobPattern>>,
}
