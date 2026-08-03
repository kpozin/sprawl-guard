//! Policy checks over classified and measured sources.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::classification::{ClassifiedSource, SourceKind, discover_sources_report};
use crate::code_line_counter::{CodeLineCounter, LocMeasurement, TokeiCodeLineCounter};
use crate::config::{Config, TraversalErrorPolicy};
use crate::resolved_limits::LimitResolver;
use crate::rust_cfg::{RustCfgTestStatus, preprocess_rust_cfg_test, rust_cfg_test_policy_info};
use crate::source_file::{ensure_source_file_size, read_source_to_string};
use crate::value::path::{normalize_under_root, resolve_root};
use crate::{CountedLoc, IncludedLineTypes, LocLimit, RelativePath, Result, SprawlError};

use super::directory_fanout::directory_fanout_checks;
use super::ratchet::{
    MeasuredRatchets, RatchetCurrentPolicy, RatchetState, directory_violations_with_ratchet,
    file_violations_with_ratchet, stale_missing_ratchets,
};
use super::report::RatchetViolationKind;
use super::report::{
    AnalysisCoverage, CheckReport, CheckViolation, CheckWarning, CounterInfo,
    DirectoryViolationKind, FileLocViolation, FileViolationKind, NonEmptyTraversalIncidents,
    ReportRoot,
};

/// Inputs for running `sprawl-guard check`.
pub struct CheckOptions<'config> {
    /// Repository root to resolve configuration-relative and CLI-supplied paths.
    pub root: PathBuf,
    /// Resolved configuration used for discovery, classification, and limits.
    pub config: &'config Config,
    /// Optional repository-relative or absolute path scopes.
    ///
    /// When empty, every discovered source file is checked. When non-empty,
    /// each scope must exist under `root`, and only files at or below at least
    /// one scope are classified and checked.
    pub paths: Vec<PathBuf>,
    /// Override `[ratchet].require_current` for this check.
    ///
    /// `None` uses config. `Some(true)` fails stale ratchet entries, and
    /// `Some(false)` permits stale entries for exploratory local checks.
    pub require_ratchet_current: Option<bool>,
    /// Override `[traversal].error_policy` for this check.
    ///
    /// `None` uses config. `Some(Warn)` is intended for exploratory local
    /// checks where descendant traversal incidents should make analysis
    /// partial without failing the run by themselves.
    pub traversal_error_policy: Option<TraversalErrorPolicy>,
}

/// Runs policy checks with the default `tokei` LOC counter.
pub fn check(options: CheckOptions<'_>) -> Result<CheckReport> {
    check_with_counter(options, &TokeiCodeLineCounter)
}

/// Runs policy checks without reading or applying the ratchet file.
///
/// Baseline and ratchet-update workflows use this to measure current policy
/// violations before deciding how the ratchet file should change.
pub fn check_without_ratchet(options: CheckOptions<'_>) -> Result<CheckReport> {
    check_with_counter_mode(options, &TokeiCodeLineCounter, RatchetApplication::Skip)
}

/// Runs policy checks with an injected LOC counter.
///
/// Tests use this entry point to make LOC measurements deterministic without
/// changing discovery and classification behavior. Production callers should
/// use [`check`] unless they are deliberately swapping the counting backend.
/// The counter's backend ID and version are copied verbatim into
/// [`CheckReport::counter`] for machine-readable output provenance.
pub fn check_with_counter(
    options: CheckOptions<'_>,
    counter: &(impl CodeLineCounter + Sync),
) -> Result<CheckReport> {
    check_with_counter_mode(options, counter, RatchetApplication::Apply)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RatchetApplication {
    Apply,
    Skip,
}

fn check_with_counter_mode(
    options: CheckOptions<'_>,
    counter: &(impl CodeLineCounter + Sync),
    ratchet_application: RatchetApplication,
) -> Result<CheckReport> {
    let root = resolve_root(&options.root)?;
    let scope = CheckScope::new(&root, options.paths)?;
    let ratchet = match ratchet_application {
        RatchetApplication::Apply => RatchetState::load_if_present(&root, options.config)?,
        RatchetApplication::Skip => None,
    };
    let ratchet_current_policy = match options
        .require_ratchet_current
        .unwrap_or(options.config.ratchet.require_current)
    {
        true => RatchetCurrentPolicy::RequireCurrent,
        false => RatchetCurrentPolicy::AllowStale,
    };
    let limit_resolver = LimitResolver::new(options.config)?;
    let discovery = discover_sources_report(
        &root,
        options.config,
        scope.paths(),
        options.traversal_error_policy,
    )?;
    let coverage = match NonEmptyTraversalIncidents::new(discovery.traversal_incidents) {
        Some(traversal_incidents) => AnalysisCoverage::Partial {
            traversal_incidents,
        },
        None => AnalysisCoverage::Complete,
    };
    let directory_checks = directory_fanout_checks(&limit_resolver, &discovery.sources)?;
    let source_checks = check_sources(
        &root,
        options.config,
        &limit_resolver,
        counter,
        discovery.sources,
    )?;
    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    let mut measured_ratchets = MeasuredRatchets::default();

    for mut source_check in source_checks {
        warnings.append(source_check.warnings_mut());
        measured_ratchets.record_file(&source_check.measurement());
        violations.extend(file_violations_with_ratchet(
            &ratchet,
            ratchet_current_policy,
            source_check,
        ));
    }
    for directory_check in directory_checks {
        measured_ratchets.record_directory(&directory_check.measurement());
        violations.extend(directory_violations_with_ratchet(
            &ratchet,
            ratchet_current_policy,
            directory_check,
        ));
    }
    if ratchet_current_policy.requires_current() {
        violations.extend(stale_missing_ratchets(&ratchet, &scope, &measured_ratchets));
    }

    violations.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    Ok(CheckReport::new(
        ReportRoot::from_resolved_path(&root),
        CounterInfo {
            backend: counter.backend_id(),
            version: counter.backend_version(),
        },
        coverage,
        warnings,
        violations,
    ))
}

impl CheckViolation {
    fn sort_key(&self) -> ViolationSortKey<'_> {
        match self {
            Self::FileLoc(violation) => {
                ViolationSortKey::new(file_violation_sort_order(violation.kind), &violation.path)
            }
            Self::DirectoryFanout(violation) => ViolationSortKey::new(
                directory_violation_sort_order(violation.kind),
                &violation.path,
            ),
            Self::Ratchet(violation) => ViolationSortKey::new(
                ratchet_violation_sort_order(violation.kind()),
                violation.path(),
            ),
        }
    }
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct ViolationSortKey<'path> {
    kind_order: u8,
    path: &'path RelativePath,
}

impl<'path> ViolationSortKey<'path> {
    fn new(kind_order: u8, path: &'path RelativePath) -> Self {
        Self { kind_order, path }
    }
}

fn file_violation_sort_order(kind: FileViolationKind) -> u8 {
    match kind {
        FileViolationKind::FileLoc => 0,
        FileViolationKind::TestFileLoc => 1,
    }
}

fn directory_violation_sort_order(kind: DirectoryViolationKind) -> u8 {
    match kind {
        DirectoryViolationKind::DirectoryFanout => 2,
        DirectoryViolationKind::TestDirectoryFanout => 3,
    }
}

fn ratchet_violation_sort_order(kind: RatchetViolationKind) -> u8 {
    match kind {
        RatchetViolationKind::FileLocIncreased | RatchetViolationKind::FileLocStale => 4,
        RatchetViolationKind::TestFileLocIncreased | RatchetViolationKind::TestFileLocStale => 5,
        RatchetViolationKind::DirectoryFanoutIncreased
        | RatchetViolationKind::DirectoryFanoutStale => 6,
        RatchetViolationKind::TestDirectoryFanoutIncreased
        | RatchetViolationKind::TestDirectoryFanoutStale => 7,
    }
}

#[derive(Clone, Debug)]
pub(super) struct FileMeasurement {
    pub(super) kind: FileViolationKind,
    pub(super) path: RelativePath,
    pub(super) loc: LocMeasurement,
    pub(super) limit: LocLimit,
}

impl FileMeasurement {
    /// Returns the aggregate selected by the resolved included line types.
    pub(super) const fn current_loc(&self) -> CountedLoc {
        self.loc.actual()
    }
}

pub(super) enum SourceCheck {
    WithinLimit {
        measurement: FileMeasurement,
        warnings: Vec<CheckWarning>,
    },
    Exceeded {
        measurement: FileMeasurement,
        violation: FileLocViolation,
        warnings: Vec<CheckWarning>,
    },
}

impl SourceCheck {
    pub(super) fn measurement(&self) -> FileMeasurement {
        match self {
            Self::WithinLimit { measurement, .. } | Self::Exceeded { measurement, .. } => {
                measurement.clone()
            }
        }
    }

    pub(super) fn warnings_mut(&mut self) -> &mut Vec<CheckWarning> {
        match self {
            Self::WithinLimit { warnings, .. } | Self::Exceeded { warnings, .. } => warnings,
        }
    }

    pub(super) fn violation_mut(&mut self) -> Option<&mut FileLocViolation> {
        match self {
            Self::WithinLimit { .. } => None,
            Self::Exceeded { violation, .. } => Some(violation),
        }
    }

    pub(super) fn has_violation(&self) -> bool {
        matches!(self, Self::Exceeded { .. })
    }

    pub(super) fn into_violation(self) -> Option<FileLocViolation> {
        match self {
            Self::WithinLimit { .. } => None,
            Self::Exceeded { violation, .. } => Some(violation),
        }
    }
}

fn check_sources(
    root: &Path,
    config: &Config,
    limits: &LimitResolver<'_>,
    counter: &(impl CodeLineCounter + Sync),
    sources: Vec<ClassifiedSource>,
) -> Result<Vec<SourceCheck>> {
    // File content reads are bounded by Rayon worker count and sorted after
    // collection, so scanning stays parallel without leaking scheduler order
    // into reports.
    sources
        .into_par_iter()
        .map(|source| check_source(root, config, limits, counter, source))
        .collect::<Result<Vec<_>>>()
}

fn check_source(
    root: &Path,
    config: &Config,
    limits: &LimitResolver<'_>,
    counter: &(impl CodeLineCounter + Sync),
    source: ClassifiedSource,
) -> Result<SourceCheck> {
    let absolute_path = root.join(source.path.to_path_buf());
    let limit = limits.file_loc_limit(&source);
    let CountedSource {
        measurement: loc,
        warnings,
    } = count_source(
        config,
        counter,
        &source,
        &absolute_path,
        limit.included_line_types,
    )?;
    let actual = loc.actual();
    let kind = match source.kind {
        SourceKind::Production => FileViolationKind::FileLoc,
        SourceKind::Test => FileViolationKind::TestFileLoc,
    };
    let measurement = FileMeasurement {
        kind,
        path: source.path.clone(),
        loc,
        limit: limit.value,
    };

    if actual.get() <= limit.value.get() {
        return Ok(SourceCheck::WithinLimit {
            measurement,
            warnings,
        });
    }

    Ok(SourceCheck::Exceeded {
        measurement,
        violation: FileLocViolation {
            kind,
            language: source.language(),
            path: source.path,
            measurement: loc,
            limit: limit.value,
            ratchet: None,
        },
        warnings,
    })
}

struct CountedSource {
    measurement: LocMeasurement,
    warnings: Vec<CheckWarning>,
}

fn count_source(
    config: &Config,
    counter: &(impl CodeLineCounter + Sync),
    source: &ClassifiedSource,
    absolute_path: &Path,
    included_line_types: IncludedLineTypes,
) -> Result<CountedSource> {
    ensure_source_file_size(absolute_path, config.traversal.max_file_bytes)?;
    let Some(preprocess_info) = rust_cfg_test_policy_info(config, source) else {
        return Ok(CountedSource {
            measurement: normalize_measurement(
                absolute_path,
                counter.count_path(absolute_path, source.language(), included_line_types)?,
                included_line_types,
            )?,
            warnings: vec![],
        });
    };
    if preprocess_info.status != RustCfgTestStatus::Parsed
        && preprocess_info.status != RustCfgTestStatus::PrefilterSkipped
    {
        return Ok(CountedSource {
            measurement: normalize_measurement(
                absolute_path,
                counter.count_path(absolute_path, source.language(), included_line_types)?,
                included_line_types,
            )?,
            warnings: vec![],
        });
    }

    let source_text = read_source_to_string(absolute_path, config.traversal.max_file_bytes)?;
    let result = preprocess_rust_cfg_test(&source_text);
    let measurement = normalize_measurement(
        absolute_path,
        counter.count_source(&result.source, source.language(), included_line_types)?,
        included_line_types,
    )?;
    let warnings = result
        .warning
        .into_iter()
        .map(|warning| CheckWarning::rust_cfg_test_parse_failed(source.path.clone(), warning))
        .collect();
    Ok(CountedSource {
        measurement,
        warnings,
    })
}

/// Measures one classified source with the production counter and check semantics.
pub(crate) fn measure_source(
    config: &Config,
    source: &ClassifiedSource,
    absolute_path: &Path,
    included_line_types: IncludedLineTypes,
) -> Result<LocMeasurement> {
    Ok(count_source(
        config,
        &TokeiCodeLineCounter,
        source,
        absolute_path,
        included_line_types,
    )?
    .measurement)
}

fn normalize_measurement(
    path: &Path,
    measurement: LocMeasurement,
    included_line_types: IncludedLineTypes,
) -> Result<LocMeasurement> {
    LocMeasurement::new(included_line_types, measurement.line_counts()).map_err(|error| {
        SprawlError::CodeLineCountOverflow {
            path: path.to_path_buf(),
            count: error.count(),
        }
    })
}
/// Normalized set of path prefixes that limit which files a check considers.
///
/// A scope is not a post-processing filter over violations. It is applied
/// during source traversal and before source classification, so out-of-scope
/// trees cannot cause unrelated traversal errors or path-dependent
/// classification errors such as ambiguous language matches. An empty scope
/// list means the whole repository is in scope.
pub(super) struct CheckScope {
    paths: Vec<RelativePath>,
}

impl CheckScope {
    /// Resolves CLI-supplied path scopes under the repository root.
    ///
    /// Relative paths stay repository-relative. Absolute paths must already be
    /// under `root`. Every non-empty scope must exist so typos cannot produce a
    /// passing empty check.
    fn new(root: &Path, paths: Vec<PathBuf>) -> Result<Self> {
        let paths = paths
            .into_iter()
            .map(|path| scope_path(root, &path))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { paths })
    }

    /// Returns normalized path scopes for traversal pruning.
    fn paths(&self) -> &[RelativePath] {
        &self.paths
    }

    pub(super) fn contains(&self, path: &RelativePath) -> bool {
        self.paths.is_empty() || self.paths.iter().any(|scope| path.is_at_or_under(scope))
    }
}

fn scope_path(root: &Path, path: &Path) -> Result<RelativePath> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if !absolute_path
        .try_exists()
        .map_err(|source| crate::SprawlError::ReadFileType {
            path: absolute_path.clone(),
            source,
        })?
    {
        return Err(crate::SprawlError::MissingCheckScope {
            path: path.to_path_buf(),
        });
    }
    if path.is_absolute() {
        let canonical_path =
            absolute_path
                .canonicalize()
                .map_err(|source| crate::SprawlError::ReadFileType {
                    path: absolute_path.clone(),
                    source,
                })?;
        return normalize_under_root(root, &canonical_path);
    }
    RelativePath::new(path)
}

#[cfg(test)]
mod tests;
