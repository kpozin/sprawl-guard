use serde::{Deserialize, Serialize};

use crate::GlobPattern;

/// Global file matching settings.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilesConfig {
    /// Glob patterns excluded before language classification.
    pub exclude: Vec<GlobPattern>,
}

pub(super) fn merge_global_file_excludes(config: &mut FilesConfig, partial: PartialFilesConfig) {
    if let Some(value) = partial.exclude {
        config.exclude = value;
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialFilesConfig {
    exclude: Option<Vec<GlobPattern>>,
}
