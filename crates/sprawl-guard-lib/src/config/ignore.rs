use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Ignore-file and hidden-path settings within the selected root.
///
/// Parent ignore files, global Git excludes, and `.git/info/exclude` do not
/// participate in source selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IgnoreConfig {
    /// Respect `.gitignore` files.
    pub respect_gitignore: bool,
    /// Respect `.ignore` files.
    pub respect_ignore: bool,
    /// Respect `.rgignore` files.
    pub respect_rgignore: bool,
    /// Respect `.fdignore` files.
    pub respect_fdignore: bool,
    /// Skip hidden files and directories.
    pub skip_hidden: bool,
}

pub(super) fn merge_ignore(config: &mut IgnoreConfig, partial: PartialIgnoreConfig) {
    if let Some(value) = partial.respect_gitignore {
        config.respect_gitignore = value;
    }
    if let Some(value) = partial.respect_ignore {
        config.respect_ignore = value;
    }
    if let Some(value) = partial.respect_rgignore {
        config.respect_rgignore = value;
    }
    if let Some(value) = partial.respect_fdignore {
        config.respect_fdignore = value;
    }
    if let Some(value) = partial.skip_hidden {
        config.skip_hidden = value;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialIgnoreConfig {
    respect_gitignore: Option<bool>,
    respect_ignore: Option<bool>,
    respect_rgignore: Option<bool>,
    respect_fdignore: Option<bool>,
    skip_hidden: Option<bool>,
}
