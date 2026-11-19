//! Source discovery and path classification.

mod bundled_filenames;
mod bundled_languages;
mod classifier;
mod explain;
mod explain_directory;
mod source_discovery;
mod traversal;
mod types;

pub use bundled_languages::{bundled_extensions, bundled_filenames};
pub use explain::{FileBytesExplanation, FileSize, explain_file_bytes, explain_path};
pub use explain_directory::{DirectFilesExplanation, DirectoryExplanation};
pub use source_discovery::discover_sources;
pub(crate) use source_discovery::discover_sources_report;
pub(crate) use types::ByteLimitedFile;
pub use types::{
    ClassifiedSource, PathClassification, SourceExplanation, SourceKind, UnreadablePath,
    UnreadablePathKind, UnreadablePathSeverity,
};
