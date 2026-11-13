use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::GlobPattern;

/// Test-code classification settings.
///
/// Source files matching global or language-specific test globs are classified
/// as test code. When `check` is true, those files are checked with
/// the test entries in `[limits]`; when false, they are omitted from checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TestsConfig {
    /// Whether files classified as test code are checked with test-code limits.
    pub check: bool,
    /// Global glob patterns that classify source files as test code.
    pub globs: Vec<GlobPattern>,
}

pub(super) fn merge_tests(config: &mut TestsConfig, partial: PartialTestsConfig) {
    if let Some(value) = partial.check {
        config.check = value;
    }
    if let Some(value) = partial.globs {
        config.globs = value;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialTestsConfig {
    check: Option<bool>,
    globs: Option<Vec<GlobPattern>>,
}
