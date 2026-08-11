mod sarif;

use std::path::Path;

use sprawl_guard_lib::CheckReport;
use sprawl_guard_lib::check::{
    CheckErrorKind, CheckViolation, CheckWarning, CheckWarningKind, DirectoryFanoutViolation,
    DirectoryViolationKind, FileLocViolation, FileViolationKind, RatchetViolation,
    RatchetViolationKind,
};
use sprawl_guard_lib::{TraversalIncident, TraversalIncidentKind};

use super::CheckFormat;
use crate::error::{CliError, Result};

#[cfg(test)]
fn missing_directory_ratchet(
    group: sprawl_guard_lib::LanguageGroupId,
    ratchet: u32,
) -> CheckViolation {
    use sprawl_guard_lib::{
        DirectoryRatchetStaleMissingViolation, RatchetCount, RatchetStaleMissingViolation,
        RatchetStaleViolation, RatchetStaleViolationKind,
    };

    CheckViolation::Ratchet(RatchetViolation::Stale(RatchetStaleViolation::Missing(
        RatchetStaleMissingViolation::Directory(DirectoryRatchetStaleMissingViolation {
            kind: RatchetStaleViolationKind::DirectoryFanoutStale,
            path: sprawl_guard_lib::RelativePath::new("src").unwrap(),
            group,
            ratchet: RatchetCount::new(ratchet),
        }),
    )))
}

pub(super) fn render_check_report(
    report: &CheckReport,
    format: CheckFormat,
    quiet: bool,
) -> Result<String> {
    match format {
        CheckFormat::Human => Ok(human_check_report(report, quiet)),
        CheckFormat::Json => serde_json::to_string_pretty(report)
            .map(|output| format!("{output}\n"))
            .map_err(|source| CliError::RenderJson { source }),
        CheckFormat::Sarif => serde_json::to_string_pretty(&sarif::sarif_check_report(report))
            .map(|output| format!("{output}\n"))
            .map_err(|source| CliError::RenderJson { source }),
    }
}

pub(super) fn traversal_failure_json_value(
    root: &Path,
    incidents: &[TraversalIncident],
) -> serde_json::Value {
    serde_json::json!({
        "status": "error",
        "root": root.display().to_string(),
        "coverage": {
            "kind": "partial",
            "traversal_incidents": incidents,
        },
        "warnings": [],
        "error": {
            "kind": CheckErrorKind::TraversalFailed,
            "message": format!(
                "traversal completed with {} incident(s)",
                incidents.len()
            ),
        },
        "violations": [],
    })
}

pub(super) fn render_traversal_failure_json(
    root: &Path,
    incidents: &[TraversalIncident],
) -> Result<String> {
    serde_json::to_string_pretty(&traversal_failure_json_value(root, incidents))
        .map(|output| format!("{output}\n"))
        .map_err(|source| CliError::RenderJson { source })
}

pub(super) fn render_traversal_failure_human(
    root: &Path,
    incidents: &[TraversalIncident],
) -> String {
    let mut output = format!(
        "Traversal failed under {} with {} incident(s):\n\n",
        root.display(),
        incidents.len()
    );
    for incident in incidents {
        output.push_str("  ");
        output.push_str(&traversal_incident_message(incident));
        output.push('\n');
    }
    output
}

fn human_check_report(report: &CheckReport, quiet: bool) -> String {
    let mut output = String::new();
    if !quiet {
        write_warnings(&mut output, report);
    }
    if !report.has_violations() {
        output.push_str("No sprawl-guard violations found.\n");
        if !quiet {
            write_scan_summary(&mut output, report);
            write_traversal_incidents(&mut output, report);
        }
        return output;
    }

    write_violation_group(
        &mut output,
        "File size budget exceeded:",
        report,
        FileViolationKind::FileLoc,
    );
    write_violation_group(
        &mut output,
        "Test file size budget exceeded:",
        report,
        FileViolationKind::TestFileLoc,
    );
    write_directory_violation_group(
        &mut output,
        "Directory fan-out budget exceeded:",
        report,
        DirectoryViolationKind::DirectoryFanout,
    );
    write_directory_violation_group(
        &mut output,
        "Test directory fan-out budget exceeded:",
        report,
        DirectoryViolationKind::TestDirectoryFanout,
    );
    write_ratchet_violation_group(&mut output, report);
    if !quiet {
        write_traversal_incidents(&mut output, report);
    }
    output
}

fn write_scan_summary(output: &mut String, report: &CheckReport) {
    let summary = report.summary();
    let files_checked = summary.files_checked.into_inner();
    let directories_checked = summary.directories_checked.into_inner();
    let languages_detected = summary.languages_detected.into_inner();
    output.push_str(&format!(
        "Checked {} source {} in {} {} across {} {}.\n",
        files_checked,
        plural(files_checked, "file", "files"),
        directories_checked,
        plural(directories_checked, "directory", "directories"),
        languages_detected,
        plural(languages_detected, "language", "languages"),
    ));
}

fn plural<'word>(count: u32, singular: &'word str, plural: &'word str) -> &'word str {
    if count == 1 { singular } else { plural }
}

fn write_warnings(output: &mut String, report: &CheckReport) {
    if report.warnings().is_empty() {
        return;
    }
    output.push_str("Warnings:\n\n");
    for warning in report.warnings() {
        output.push_str("  ");
        output.push_str(&warning_message(warning));
        output.push('\n');
    }
    output.push('\n');
}

fn warning_message(warning: &CheckWarning) -> String {
    match warning.kind {
        CheckWarningKind::RustCfgTestParseFailed => format!(
            "Rust cfg(test) preprocessing failed for {}; counted without cfg(test) exclusion: {}",
            warning.path, warning.detail
        ),
    }
}

fn write_traversal_incidents(output: &mut String, report: &CheckReport) {
    if report.traversal_incidents().is_empty() {
        return;
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str("Traversal warnings:\n\n");
    for incident in report.traversal_incidents() {
        output.push_str("  ");
        output.push_str(&traversal_incident_message(incident));
        output.push('\n');
    }
}

fn traversal_incident_message(incident: &TraversalIncident) -> String {
    let path = incident
        .path
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| ".".to_owned());
    format!(
        "{} at {}: {}",
        traversal_incident_kind_message(incident.kind),
        path,
        incident.detail
    )
}

fn traversal_incident_kind_message(kind: TraversalIncidentKind) -> &'static str {
    match kind {
        TraversalIncidentKind::ReadDirFailed => "failed to read directory",
        TraversalIncidentKind::FileTypeFailed => "failed to read file type",
        TraversalIncidentKind::SymlinkLoop => "symlink loop found",
        TraversalIncidentKind::BrokenSymlink => "broken symlink",
        TraversalIncidentKind::IgnoreParseFailed => "failed to parse ignore rule",
        TraversalIncidentKind::EntryLimitExceeded => "traversal entry limit exceeded",
    }
}

fn write_violation_group(
    output: &mut String,
    header: &str,
    report: &CheckReport,
    kind: FileViolationKind,
) {
    let violations = report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::FileLoc(violation) if violation.kind == kind => Some(violation),
            CheckViolation::FileLoc(_)
            | CheckViolation::DirectoryFanout(_)
            | CheckViolation::Ratchet(_) => None,
        })
        .collect::<Vec<_>>();
    write_non_empty_violation_group(output, header, violations, violation_message);
}

fn violation_message(violation: &FileLocViolation) -> String {
    let metric = "configured LOC";
    let included_line_types = violation.measurement.included_line_types();
    format!(
        "  {}\n    language: {}\n    included line types: {included_line_types}\n    {metric}: {} / {}\n\n",
        violation.path,
        violation.language,
        violation.measurement.actual().get(),
        violation.limit
    )
}

fn write_directory_violation_group(
    output: &mut String,
    header: &str,
    report: &CheckReport,
    kind: DirectoryViolationKind,
) {
    let violations = report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::DirectoryFanout(violation) if violation.kind == kind => Some(violation),
            CheckViolation::DirectoryFanout(_)
            | CheckViolation::FileLoc(_)
            | CheckViolation::Ratchet(_) => None,
        })
        .collect::<Vec<_>>();
    write_non_empty_violation_group(output, header, violations, directory_violation_message);
}

fn write_non_empty_violation_group<T>(
    output: &mut String,
    header: &str,
    violations: Vec<&T>,
    violation_message: impl Fn(&T) -> String,
) {
    if violations.is_empty() {
        return;
    }

    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(header);
    output.push_str("\n\n");
    for violation in violations {
        output.push_str(&violation_message(violation));
    }
}

fn write_ratchet_violation_group(output: &mut String, report: &CheckReport) {
    let violations = report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::Ratchet(violation) => Some(violation),
            CheckViolation::FileLoc(_) | CheckViolation::DirectoryFanout(_) => None,
        })
        .collect::<Vec<_>>();
    if violations.is_empty() {
        return;
    }

    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str("Ratchet failures:\n\n");
    for violation in violations {
        output.push_str(&ratchet_violation_message(violation));
    }
}

fn ratchet_violation_message(violation: &RatchetViolation) -> String {
    let actual = violation
        .actual()
        .map(|value| value.get().to_string())
        .unwrap_or_else(|| "not measured".to_owned());
    let limit = violation
        .limit()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned());
    let group = violation
        .group()
        .map(|group| format!("    group: {group}\n"))
        .unwrap_or_default();
    let included_line_types = violation
        .loc_measurement()
        .map(|measurement| {
            format!(
                "    included line types: {}\n",
                measurement.included_line_types()
            )
        })
        .unwrap_or_default();
    format!(
        "  {}\n    kind: {}\n{group}{included_line_types}    actual: {}\n    limit: {}\n    ratchet: {}\n\n",
        violation.path(),
        ratchet_kind_message(violation.kind()),
        actual,
        limit,
        violation.ratchet().get()
    )
}

fn ratchet_kind_message(kind: RatchetViolationKind) -> &'static str {
    match kind {
        RatchetViolationKind::FileLocIncreased => "file LOC increased",
        RatchetViolationKind::TestFileLocIncreased => "test file LOC increased",
        RatchetViolationKind::DirectoryFanoutIncreased => "directory fan-out increased",
        RatchetViolationKind::TestDirectoryFanoutIncreased => "test directory fan-out increased",
        RatchetViolationKind::FileLocStale => "file LOC ratchet is stale",
        RatchetViolationKind::TestFileLocStale => "test file LOC ratchet is stale",
        RatchetViolationKind::DirectoryFanoutStale => "directory fan-out ratchet is stale",
        RatchetViolationKind::TestDirectoryFanoutStale => "test directory fan-out ratchet is stale",
    }
}

fn directory_violation_message(violation: &DirectoryFanoutViolation) -> String {
    let metric = match violation.kind {
        DirectoryViolationKind::DirectoryFanout => "leaf source files",
        DirectoryViolationKind::TestDirectoryFanout => "leaf test files",
    };
    let path = if violation.path.as_str().is_empty() {
        "."
    } else {
        violation.path.as_str()
    };
    let files = violation
        .files
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n      ");
    format!(
        "  {}\n    group: {}\n    {metric}: {} / {}\n    files:\n      {files}\n\n",
        path, violation.group, violation.actual, violation.limit
    )
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use serde_json::json;
    use sprawl_guard_lib::{
        CheckViolation, CheckWarning, CheckWarningKind, CheckedDirectoryCount, CheckedFileCount,
        DetectedLanguageCount, DiagnosticDetail, DirectoryFanoutViolation,
        DirectoryRatchetStaleMeasuredViolation, DirectoryViolationKind, FileLocViolation,
        FileRatchetIncreasedViolation, LanguageGroupId, LeafFiles, LineCount, LocLimit,
        NonEmptyTraversalIncidents, RatchetCount, RatchetIncreasedViolation, RatchetLimit,
        RatchetStaleMeasuredViolation, RatchetStaleViolation, RatchetViolation, RelativePath,
        ScanSummary, TraversalIncidentSeverity,
    };

    use super::*;

    fn report_root() -> sprawl_guard_lib::ReportRoot {
        sprawl_guard_lib::ReportRoot::trusted("/repo")
    }

    fn counter_info() -> sprawl_guard_lib::check::CounterInfo {
        sprawl_guard_lib::check::CounterInfo {
            backend: "tokei",
            version: "14.0.0",
        }
    }

    fn scan_summary() -> ScanSummary {
        ScanSummary::new(
            CheckedFileCount::new(3),
            CheckedDirectoryCount::new(2),
            DetectedLanguageCount::new(1),
        )
    }

    fn report_with(violations: Vec<FileLocViolation>) -> CheckReport {
        CheckReport::complete(
            report_root(),
            counter_info(),
            scan_summary(),
            vec![],
            violations
                .into_iter()
                .map(CheckViolation::FileLoc)
                .collect(),
        )
    }

    fn violation(kind: FileViolationKind, path: &str) -> FileLocViolation {
        FileLocViolation {
            kind,
            path: RelativePath::new(path).unwrap(),
            language: sprawl_guard_lib::LanguageId::new("Rust").unwrap(),
            measurement: sprawl_guard_lib::code_line_counter::LocMeasurement::new(
                sprawl_guard_lib::IncludedLineTypes::Code,
                sprawl_guard_lib::LineCounts::new(
                    LineCount::new(3),
                    LineCount::new(0),
                    LineCount::new(0),
                ),
            )
            .unwrap(),
            limit: LocLimit::new(2).unwrap(),
            ratchet: None,
        }
    }

    fn directory_violation(kind: DirectoryViolationKind, path: &str) -> DirectoryFanoutViolation {
        DirectoryFanoutViolation {
            kind,
            group: sprawl_guard_lib::LanguageGroupId::Source,
            path: RelativePath::new(path).unwrap(),
            actual: LeafFiles::new(3).unwrap(),
            limit: LeafFiles::new(2).unwrap(),
            ratchet: None,
            files: vec![
                RelativePath::new(format!("{path}/a.rs")).unwrap(),
                RelativePath::new(format!("{path}/b.rs")).unwrap(),
                RelativePath::new(format!("{path}/c.rs")).unwrap(),
            ],
        }
    }

    fn rust_cfg_warning() -> CheckWarning {
        CheckWarning {
            kind: CheckWarningKind::RustCfgTestParseFailed,
            path: RelativePath::new("src/lib.rs").unwrap(),
            detail: DiagnosticDetail::new("expected identifier"),
        }
    }

    fn report_with_rust_cfg_warning() -> CheckReport {
        CheckReport::complete(
            report_root(),
            counter_info(),
            scan_summary(),
            vec![rust_cfg_warning()],
            vec![],
        )
    }

    mod when_rendering_human_check_output {
        use super::*;

        #[test]
        fn it_groups_file_and_test_file_violations() {
            let report = report_with(vec![
                violation(FileViolationKind::FileLoc, "src/lib.rs"),
                violation(FileViolationKind::TestFileLoc, "tests/integration.rs"),
            ]);

            let output = human_check_report(&report, false);

            assert!(output.contains("File size budget exceeded:"));
            assert!(output.contains("src/lib.rs"));
            assert!(output.contains("included line types: code"));
            assert!(output.contains("configured LOC: 3 / 2"));
            assert!(output.contains("Test file size budget exceeded:"));
            assert!(output.contains("tests/integration.rs"));
            assert!(output.contains("configured LOC: 3 / 2"));
        }

        #[test]
        fn it_groups_production_and_test_directory_violations() {
            let report = CheckReport::complete(
                report_root(),
                counter_info(),
                scan_summary(),
                vec![],
                vec![
                    CheckViolation::DirectoryFanout(directory_violation(
                        DirectoryViolationKind::DirectoryFanout,
                        "src",
                    )),
                    CheckViolation::DirectoryFanout(directory_violation(
                        DirectoryViolationKind::TestDirectoryFanout,
                        "tests",
                    )),
                ],
            );

            let output = human_check_report(&report, false);

            assert!(output.contains("Directory fan-out budget exceeded:"));
            assert!(output.contains("src"));
            assert!(output.contains("group: source"));
            assert!(output.contains("leaf source files: 3 / 2"));
            assert!(output.contains("src/a.rs"));
            assert!(output.contains("Test directory fan-out budget exceeded:"));
            assert!(output.contains("leaf test files: 3 / 2"));
            assert!(output.contains("tests/a.rs"));
        }

        #[test]
        fn it_distinguishes_missing_directory_ratchets_for_groups_at_the_same_path() {
            let report = CheckReport::complete(
                report_root(),
                counter_info(),
                scan_summary(),
                vec![],
                vec![
                    missing_directory_ratchet(LanguageGroupId::Source, 4),
                    missing_directory_ratchet(LanguageGroupId::Documentation, 5),
                ],
            );

            let output = human_check_report(&report, false);

            assert_eq!(
                output,
                indoc! {"
                    Ratchet failures:

                      src
                        kind: directory fan-out ratchet is stale
                        group: source
                        actual: not measured
                        limit: none
                        ratchet: 4

                      src
                        kind: directory fan-out ratchet is stale
                        group: documentation
                        actual: not measured
                        limit: none
                        ratchet: 5

                "}
            );
        }

        #[test]
        fn it_identifies_the_group_and_unlimited_limit_for_a_measured_directory_ratchet() {
            let report = CheckReport::complete(
                report_root(),
                counter_info(),
                scan_summary(),
                vec![],
                vec![CheckViolation::Ratchet(RatchetViolation::Stale(
                    RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::Directory(
                        DirectoryRatchetStaleMeasuredViolation {
                            kind: DirectoryViolationKind::DirectoryFanout,
                            path: RelativePath::new("docs").unwrap(),
                            group: LanguageGroupId::Documentation,
                            actual: RatchetCount::new(2),
                            limit: RatchetLimit::Unlimited,
                            ratchet: RatchetCount::new(3),
                        },
                    )),
                ))],
            );

            let output = human_check_report(&report, false);

            assert!(output.contains("group: documentation"));
            assert!(output.contains("limit: unlimited"));
        }

        #[test]
        fn it_reports_included_line_types_for_a_measured_file_ratchet() {
            let report = CheckReport::complete(
                report_root(),
                counter_info(),
                scan_summary(),
                vec![],
                vec![CheckViolation::Ratchet(RatchetViolation::Increased(
                    RatchetIncreasedViolation::File(FileRatchetIncreasedViolation {
                        kind: FileViolationKind::FileLoc,
                        path: RelativePath::new("src/lib.rs").unwrap(),
                        measurement: sprawl_guard_lib::code_line_counter::LocMeasurement::new(
                            sprawl_guard_lib::IncludedLineTypes::CodeAndComments,
                            sprawl_guard_lib::LineCounts::new(
                                LineCount::new(3),
                                LineCount::new(2),
                                LineCount::new(1),
                            ),
                        )
                        .unwrap(),
                        limit: RatchetCount::new(2),
                        ratchet: RatchetCount::new(4),
                    }),
                ))],
            );

            let output = human_check_report(&report, false);

            assert!(output.contains("included line types: code, comments"));
            assert!(output.contains("actual: 5"));
        }

        #[test]
        fn it_reports_success_when_there_are_no_violations() {
            let output = human_check_report(&report_with(vec![]), false);

            assert_eq!(
                output,
                concat!(
                    "No sprawl-guard violations found.\n",
                    "Checked 3 source files in 2 directories across 1 language.\n",
                )
            );
        }

        #[test]
        fn it_includes_warnings_before_the_status() {
            let report = report_with_rust_cfg_warning();

            let output = human_check_report(&report, false);

            assert_eq!(
                output,
                indoc! {"
                    Warnings:

                      Rust cfg(test) preprocessing failed for src/lib.rs; counted without cfg(test) exclusion: expected identifier

                    No sprawl-guard violations found.
                    Checked 3 source files in 2 directories across 1 language.
                "}
            );
        }

        #[test]
        fn it_suppresses_warnings_when_quiet() {
            let report = report_with_rust_cfg_warning();

            let output = human_check_report(&report, true);

            assert_eq!(output, "No sprawl-guard violations found.\n");
        }

        #[test]
        fn it_reports_traversal_incidents_after_the_success_status() {
            let report = CheckReport::partial(
                report_root(),
                counter_info(),
                scan_summary(),
                vec![],
                NonEmptyTraversalIncidents::new(vec![TraversalIncident {
                    kind: TraversalIncidentKind::ReadDirFailed,
                    severity: TraversalIncidentSeverity::Warning,
                    path: Some(RelativePath::new("generated").unwrap()),
                    detail: DiagnosticDetail::new("permission denied"),
                }])
                .unwrap(),
                vec![],
            );

            let output = human_check_report(&report, false);

            assert_eq!(
                output,
                indoc! {"
                    No sprawl-guard violations found.
                    Checked 3 source files in 2 directories across 1 language.

                    Traversal warnings:

                      failed to read directory at generated: permission denied
                "}
            );
        }
    }

    mod when_rendering_json_check_output {
        use super::*;

        #[test]
        fn it_uses_the_stable_report_schema() {
            let report = report_with(vec![violation(FileViolationKind::FileLoc, "src/lib.rs")]);

            let output = serde_json::to_value(&report).unwrap();

            assert_eq!(
                output,
                json!({
                    "status": "failed",
                    "root": "/repo",
                    "coverage": {
                        "kind": "complete"
                    },
                    "summary": {
                        "files_checked": 3,
                        "directories_checked": 2,
                        "languages_detected": 1
                    },
                    "counter": {
                        "backend": "tokei",
                        "version": "14.0.0"
                    },
                    "warnings": [],
                    "violations": [
                        {
                            "kind": "file_loc",
                            "path": "src/lib.rs",
                            "language": "Rust",
                            "included_line_types": ["code"],
                            "line_counts": {
                                "code": 3,
                                "comments": 0,
                                "blank_lines": 0
                            },
                            "actual": 3,
                            "limit": 2,
                            "ratchet": null
                        }
                    ]
                })
            );
        }

        #[test]
        fn it_serializes_structured_warnings() {
            let report = report_with_rust_cfg_warning();

            let output = serde_json::to_value(&report).unwrap();

            assert_eq!(
                output["warnings"],
                json!([
                    {
                        "kind": "rust_cfg_test_parse_failed",
                        "path": "src/lib.rs",
                        "detail": "expected identifier"
                    }
                ])
            );
        }
    }
}
