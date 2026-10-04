//! Source discovery and path classification.

use std::path::Path;

use crate::config::{Config, TraversalErrorPolicy};
use crate::value::path::{normalize_under_root, resolve_root};
use crate::{DiagnosticDetail, RelativePath, Result, SprawlError};

use super::classifier::Classifier;
use super::traversal::{
    collect_walk_entry, file_type_incident, is_source_file_candidate, walk_sources_scoped,
};
use super::types::{ClassifiedSource, SourceDiscoveryReport, TraversalIncidentSeverity};

/// Discovers included source files under the repository root.
///
/// Discovery uses the configured ignore policy first, then applies global file
/// exclusions, per-language include/exclude patterns, and path-based
/// production/test classification. Ignored paths, non-source files, globally
/// excluded files, language-excluded files, and disabled test-code files are
/// omitted from the result. Ambiguous language matches are returned as errors
/// because checks need one definitive language-specific policy for each source
/// file. Paths returned from this function are repository-relative and sorted
/// deterministically for stable reports. Because this simple API cannot return
/// traversal incidents, any recoverable traversal incident is reported as an
/// error even when config would allow `check` to downgrade it to a warning.
pub fn discover_sources(root: &Path, config: &Config) -> Result<Vec<ClassifiedSource>> {
    discover_sources_scoped(root, config, &[])
}

/// Discovers source files below caller-supplied repository-relative scopes.
///
/// Scope pruning happens during traversal. That lets path-scoped checks avoid
/// visiting unrelated subtrees before those files can produce traversal errors
/// or path-dependent classification errors.
pub(crate) fn discover_sources_scoped(
    root: &Path,
    config: &Config,
    scopes: &[RelativePath],
) -> Result<Vec<ClassifiedSource>> {
    Ok(discover_sources_report(root, config, scopes, Some(TraversalErrorPolicy::Fail))?.sources)
}

/// Discovers source files and recoverable traversal incidents.
pub(crate) fn discover_sources_report(
    root: &Path,
    config: &Config,
    scopes: &[RelativePath],
    error_policy_override: Option<TraversalErrorPolicy>,
) -> Result<SourceDiscoveryReport> {
    let root = resolve_root(root)?;
    let classifier = Classifier::new(config)?;
    let mut sources = Vec::new();
    let mut traversal_incidents = Vec::new();
    let mut visited_entries = 0_u32;
    let error_policy = error_policy_override.unwrap_or(config.traversal.error_policy);
    let severity = incident_severity(error_policy);

    for result in walk_sources_scoped(&root, config, scopes) {
        let Some(entry) = collect_walk_entry(
            &root,
            config,
            severity,
            &mut visited_entries,
            &mut traversal_incidents,
            result,
        )?
        else {
            continue;
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        let is_source_candidate = match is_source_file_candidate(entry.path(), file_type, config) {
            Ok(value) => value,
            Err(SprawlError::ReadFileType { path, source }) => {
                traversal_incidents.push(file_type_incident(
                    &root,
                    severity,
                    &path,
                    DiagnosticDetail::new(source.to_string()),
                ));
                continue;
            }
            Err(source) => return Err(source),
        };
        if !is_source_candidate {
            continue;
        }
        let relative = normalize_under_root(&root, entry.path())?;
        if let Some(source) = classifier.included_source(relative)? {
            sources.push(source);
        }
    }

    sources.sort_by(|left, right| left.path.cmp(&right.path));
    traversal_incidents.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    if error_policy == TraversalErrorPolicy::Fail && !traversal_incidents.is_empty() {
        return Err(SprawlError::TraversalFailed {
            root,
            incidents: traversal_incidents,
        });
    }
    Ok(SourceDiscoveryReport {
        sources,
        traversal_incidents,
    })
}

fn incident_severity(policy: TraversalErrorPolicy) -> TraversalIncidentSeverity {
    match policy {
        TraversalErrorPolicy::Fail => TraversalIncidentSeverity::Error,
        TraversalErrorPolicy::Warn => TraversalIncidentSeverity::Warning,
    }
}

#[cfg(test)]
mod matcher_tests;

#[cfg(test)]
#[path = "non_utf8_discovery_tests.rs"]
mod non_utf8_tests;

#[cfg(test)]
mod tests;
