//! Resolved configuration loading and domain types.

mod defaults;
mod files;
mod ignore;
mod include;
mod language_groups;
mod languages;
mod limits;
mod load;
mod output;
mod overrides;
mod ratchet;
mod tests;
mod traversal;
mod types;

pub use defaults::{built_in_config, supported_language_ids};
pub use files::FilesConfig;
pub use ignore::IgnoreConfig;
pub use include::ConfigInclude;
pub use language_groups::{
    CustomLanguageGroupConfig, DocumentationLanguageGroupConfig, LanguageGroupLimits,
    LanguageGroupMembers, LanguageGroupMembersError, LanguageGroupsConfig, LanguageGroupsError,
};
pub use languages::{
    CommonLanguageConfig, GenericLanguageConfig, LanguageConfig, LanguageLimits,
    LanguageMaxLinesPerFileLimits, LanguageMode, LanguageSpecificConfig, RustCfgTestExclusion,
    RustLanguageConfig,
};
pub use limits::{Limits, MaxLeafFilesPerDirectoryLimits, MaxLinesPerFileLimits};
pub use load::{LoadOptions, LoadedConfig, load_resolved_config};
pub use output::{ColorMode, OutputConfig, OutputFormat};
pub use overrides::{
    OverrideLimits, OverridePaths, OverridePathsError, OverrideReason, OverrideReasonError,
    PathOverride,
};
pub use ratchet::RatchetConfig;
pub use tests::TestsConfig;
pub use traversal::{
    SourceFileByteLimit, SourceFileByteLimitError, SymlinkTraversal, TraversalConfig,
    TraversalDepth, TraversalDepthError, TraversalEntryLimit, TraversalEntryLimitError,
    TraversalErrorPolicy,
};
pub use types::Config;
