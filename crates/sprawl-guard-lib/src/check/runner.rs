//! Checks over classified and measured sources.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::classification::{
    ClassifiedSource, SourceKind, UnreadablePath, UnreadablePathKind, UnreadablePathSeverity,
    discover_sources_report,
};
use crate::code_line_counter::{CodeLineCounter, LineMeasurement, TokeiCodeLineCounter};
use crate::config::{Config, ResolvedConfig, UnreadablePathPolicy};
use crate::resolved_limits::LimitResolver;
use crate::rust_cfg::{
    RustCfgTestInfo, RustCfgTestStatus, preprocess_rust_cfg_test, rust_cfg_test_policy_info,
};
use crate::source_file::{
    ParsedCodeFileSize, ensure_parsed_code_file_size, parsed_code_file_size, read_source_to_string,
};
use crate::value::path::resolve_root;
use crate::{CountLines, DiagnosticDetail, RelativePath, Result, SprawlError};

use super::directory_direct_files::directory_direct_files_checks;
use super::ratchet::{
    MeasuredRatchets, RatchetCurrentPolicy, RatchetState, directory_violations_with_ratchet,
    file_violations_with_ratchet, stale_missing_ratchets,
};
use super::report::{
    AnalysisCoverage, CheckContext, CheckReport, CheckWarning, CounterInfo, FileLinesViolation,
    FileViolationKind, RatchetEntryCount, ReportRoot, UncountedFile,
};

mod selected_paths;
mod source_check;
mod summary;
mod violation_order;

pub use selected_paths::{NonEmptyCheckPaths, SelectedPaths};
pub(super) use source_check::{FileMeasurement, SourceCheck};
use summary::summarize_scan;
use violation_order::sort_violations;

/// Inputs for running `sprawl-guard check`.
pub struct CheckOptions<'config> {
    /// Repository root to resolve configuration-relative and CLI-supplied paths.
    pub root: PathBuf,
    /// Resolved configuration and the source from which it was assembled.
    pub policy: ResolvedConfig<'config>,
    /// Optional repository-relative or absolute selected paths.
    ///
    /// When empty, or when a path resolves to `root`, every discovered source file is checked.
    /// Otherwise, each selected path must exist under `root`, and only files at or below at least one selected path
    /// are classified and checked.
    pub paths: Vec<PathBuf>,
    /// Override `[ratchet].require_current` for this check.
    ///
    /// `None` uses config. `Some(true)` fails stale ratchet entries, and
    /// `Some(false)` permits stale entries for exploratory local checks.
    pub require_ratchet_current: Option<bool>,
    /// Override `[traversal].unreadable_paths` for this check.
    ///
    /// `None` uses config. The check reports unreadable paths and uncounted files either way;
    /// `Some(Fail)` gives the report [`CheckStatus::Error`](super::CheckStatus::Error) when there
    /// are any, and `Some(Warn)` lets it pass or fail on its violations alone.
    pub unreadable_paths: Option<UnreadablePathPolicy>,
}

/// Runs checks with the default `tokei` line counter.
pub fn check(options: CheckOptions<'_>) -> Result<CheckReport> {
    check_with_counter(options, &TokeiCodeLineCounter)
}

/// Runs checks without reading or applying the ratchet file.
///
/// The `update` and `tighten` workflows use this to measure current violation before
/// editing the ratchet file.
pub fn check_without_ratchet(options: CheckOptions<'_>) -> Result<CheckReport> {
    check_with_counter_mode(options, &TokeiCodeLineCounter, RatchetApplication::Skip)
}

/// Runs checks with an injected line counter.
///
/// Tests use this entry point to make line measurements deterministic without changing discovery
/// and classification behavior. Production callers use [`check`] unless they deliberately replace
/// the counting backend.
///
/// The counter's backend ID and version are copied verbatim into
/// [`CheckReport::counter`] so machine-readable output names the line counter.
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
    let config = options.policy.config();
    let config_source = options.policy.source().clone();
    let root = resolve_root(&options.root)?;
    let selected_paths = SelectedPaths::resolve(&root, options.paths)?;
    let ratchet = match ratchet_application {
        RatchetApplication::Apply => RatchetState::load_if_present(&root, config)?,
        RatchetApplication::Skip => None,
    };
    let ratchet_current_policy = match options
        .require_ratchet_current
        .unwrap_or(config.ratchet.require_current)
    {
        true => RatchetCurrentPolicy::RequireCurrent,
        false => RatchetCurrentPolicy::AllowStale,
    };
    let limit_resolver = LimitResolver::new(config)?;
    let gap_policy = options
        .unreadable_paths
        .unwrap_or(config.traversal.unreadable_paths);
    let discovery =
        discover_sources_report(&root, config, selected_paths.paths(), Some(gap_policy))?;
    let directory_checks = directory_direct_files_checks(&limit_resolver, &discovery.sources)?;
    let summary = summarize_scan(
        &discovery.sources,
        &directory_checks,
        ratchet
            .as_ref()
            .map_or(RatchetEntryCount::new(0), RatchetState::entry_count),
    );
    let sources = SourceCheckInputs {
        root: &root,
        config,
        limits: &limit_resolver,
        gap_severity: gap_policy.into(),
    };
    let source_outcomes = check_sources(&sources, counter, discovery.sources)?;
    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    let mut uncounted_files = Vec::new();
    let mut unreadable_paths = discovery.unreadable_paths;
    let mut measured_ratchets = MeasuredRatchets::default();

    for source_outcome in source_outcomes {
        match source_outcome {
            SourceOutcome::Counted(mut source_check) => {
                warnings.append(source_check.warnings_mut());
                measured_ratchets.record_file(&source_check.measurement());
                violations.extend(file_violations_with_ratchet(
                    &ratchet,
                    ratchet_current_policy,
                    source_check,
                ));
            }
            SourceOutcome::Uncounted(file) => {
                measured_ratchets.record_uncounted_file(&file.path, file.kind);
                uncounted_files.push(file);
            }
            SourceOutcome::Unreadable { kind, path, detail } => {
                measured_ratchets.record_uncounted_file(&path, kind);
                unreadable_paths.push(UnreadablePath {
                    kind: UnreadablePathKind::ReadFileFailed,
                    severity: sources.gap_severity,
                    path: Some(path),
                    detail,
                });
            }
        }
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
        violations.extend(stale_missing_ratchets(
            &ratchet,
            &selected_paths,
            &measured_ratchets,
        ));
    }

    sort_violations(&mut violations);
    unreadable_paths.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    Ok(CheckReport::new(
        CheckContext::new(
            ReportRoot::from_resolved_path(&root),
            selected_paths,
            config_source,
        ),
        CounterInfo {
            backend: counter.backend_id(),
            version: counter.backend_version(),
        },
        AnalysisCoverage::new(unreadable_paths, uncounted_files),
        summary,
        warnings,
        violations,
    ))
}

/// What one source file's check produced.
pub(super) enum SourceOutcome {
    /// The file's lines were counted and checked against its line limit.
    Counted(SourceCheck),
    /// The file is too large to count, so its line limit went unchecked.
    Uncounted(UncountedFile),
    /// The file failed to read, so its line limit went unchecked.
    Unreadable {
        kind: FileViolationKind,
        path: RelativePath,
        detail: DiagnosticDetail,
    },
}

/// Settings shared by every source file's check.
struct SourceCheckInputs<'inputs> {
    root: &'inputs Path,
    config: &'inputs Config,
    limits: &'inputs LimitResolver<'inputs>,
    /// Severity of an uncounted file, from `traversal.unreadable_paths`.
    gap_severity: UnreadablePathSeverity,
}

fn check_sources(
    inputs: &SourceCheckInputs<'_>,
    counter: &(impl CodeLineCounter + Sync),
    sources: Vec<ClassifiedSource>,
) -> Result<Vec<SourceOutcome>> {
    // File content reads are bounded by Rayon worker count and sorted after
    // collection, so scanning stays parallel without leaking scheduler order
    // into reports.
    sources
        .into_par_iter()
        .map(|source| check_source(inputs, counter, source))
        .collect::<Result<Vec<_>>>()
}

/// Checks one source file, turning a failure to read it into a gap in the analysis.
fn check_source(
    inputs: &SourceCheckInputs<'_>,
    counter: &(impl CodeLineCounter + Sync),
    source: ClassifiedSource,
) -> Result<SourceOutcome> {
    let kind = match source.kind {
        SourceKind::Production => FileViolationKind::FileLines,
        SourceKind::Test => FileViolationKind::TestFileLines,
    };
    let path = source.path.clone();
    match check_readable_source(inputs, counter, source, kind) {
        Err(SprawlError::ReadSource { source, .. } | SprawlError::ReadFileType { source, .. }) => {
            Ok(SourceOutcome::Unreadable {
                kind,
                path,
                detail: DiagnosticDetail::new(source.to_string()),
            })
        }
        outcome => outcome,
    }
}

fn check_readable_source(
    inputs: &SourceCheckInputs<'_>,
    counter: &(impl CodeLineCounter + Sync),
    source: ClassifiedSource,
    kind: FileViolationKind,
) -> Result<SourceOutcome> {
    let config = inputs.config;
    let absolute_path = inputs.root.join(source.path.to_path_buf());
    let byte_limit = config.traversal.max_parsed_code_file_bytes;
    if let ParsedCodeFileSize::TooLarge(size) = parsed_code_file_size(&absolute_path, byte_limit)? {
        return Ok(SourceOutcome::Uncounted(UncountedFile {
            kind,
            severity: inputs.gap_severity,
            path: source.path,
            size,
            limit: byte_limit,
        }));
    }
    let limit = inputs.limits.file_lines_limit(&source);
    let CountedSource {
        measurement: lines,
        warnings,
        ..
    } = count_source(config, counter, &source, &absolute_path, limit.count_lines)?;
    let actual = lines.actual();
    let measurement = FileMeasurement {
        kind,
        path: source.path.clone(),
        lines,
        limit: limit.value,
    };

    if actual.get() <= limit.value.get() {
        return Ok(SourceOutcome::Counted(SourceCheck::WithinLimit {
            measurement,
            warnings,
        }));
    }

    Ok(SourceOutcome::Counted(SourceCheck::Exceeded {
        measurement,
        violation: FileLinesViolation {
            kind,
            language: source.language(),
            path: source.path,
            measurement: lines,
            limit: limit.value,
            ratchet: None,
        },
        warnings,
    }))
}

struct CountedSource {
    measurement: LineMeasurement,
    rust_cfg_test: Option<RustCfgTestInfo>,
    warnings: Vec<CheckWarning>,
}

fn count_source(
    config: &Config,
    counter: &(impl CodeLineCounter + Sync),
    source: &ClassifiedSource,
    absolute_path: &Path,
    count_lines: CountLines,
) -> Result<CountedSource> {
    let Some(preprocess_info) = rust_cfg_test_policy_info(config, source) else {
        return Ok(CountedSource {
            measurement: normalize_measurement(
                absolute_path,
                counter.count_path(absolute_path, source.language(), count_lines)?,
                count_lines,
            )?,
            rust_cfg_test: None,
            warnings: vec![],
        });
    };
    if preprocess_info.status != RustCfgTestStatus::Parsed
        && preprocess_info.status != RustCfgTestStatus::PrefilterSkipped
    {
        return Ok(CountedSource {
            measurement: normalize_measurement(
                absolute_path,
                counter.count_path(absolute_path, source.language(), count_lines)?,
                count_lines,
            )?,
            rust_cfg_test: Some(preprocess_info),
            warnings: vec![],
        });
    }

    let source_text =
        read_source_to_string(absolute_path, config.traversal.max_parsed_code_file_bytes)?;
    let result = preprocess_rust_cfg_test(&source_text);
    let measurement = normalize_measurement(
        absolute_path,
        counter.count_source(&result.source, source.language(), count_lines)?,
        count_lines,
    )?;
    let rust_cfg_test = Some(result.info);
    let warnings = result
        .warning
        .into_iter()
        .map(|warning| CheckWarning::rust_cfg_test_parse_failed(source.path.clone(), warning))
        .collect();
    Ok(CountedSource {
        measurement,
        rust_cfg_test,
        warnings,
    })
}

/// Source measurement plus preprocessing metadata from the same read.
pub(crate) struct SourceMeasurement {
    /// line measurements produced for the source.
    pub(crate) measurement: LineMeasurement,
    /// Rust `cfg(test)` preprocessing metadata when `exclude_cfg_test` applies.
    pub(crate) rust_cfg_test: Option<RustCfgTestInfo>,
}

/// Measures one classified source with the production counter and check semantics.
///
/// A file larger than `traversal.max_parsed_code_file_bytes` is an error here, since there is
/// no measurement to return.
pub(crate) fn measure_source(
    config: &Config,
    source: &ClassifiedSource,
    absolute_path: &Path,
    count_lines: CountLines,
) -> Result<SourceMeasurement> {
    ensure_parsed_code_file_size(absolute_path, config.traversal.max_parsed_code_file_bytes)?;
    let counted = count_source(
        config,
        &TokeiCodeLineCounter,
        source,
        absolute_path,
        count_lines,
    )?;
    Ok(SourceMeasurement {
        measurement: counted.measurement,
        rust_cfg_test: counted.rust_cfg_test,
    })
}

fn normalize_measurement(
    path: &Path,
    measurement: LineMeasurement,
    count_lines: CountLines,
) -> Result<LineMeasurement> {
    LineMeasurement::new(count_lines, measurement.line_counts()).map_err(|error| {
        SprawlError::CodeLineCountOverflow {
            path: path.to_path_buf(),
            count: error.count(),
        }
    })
}
#[cfg(test)]
mod tests;
