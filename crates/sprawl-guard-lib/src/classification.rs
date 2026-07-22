//! Source discovery and path classification.

mod classifier;
mod explain;
mod source_discovery;
mod traversal;
mod types;

pub use explain::explain_path;
pub use source_discovery::discover_sources;
pub(crate) use source_discovery::discover_sources_report;
pub use types::{
    ClassifiedSource, PathClassification, SourceExplanation, SourceKind, TraversalIncident,
    TraversalIncidentKind, TraversalIncidentSeverity,
};
