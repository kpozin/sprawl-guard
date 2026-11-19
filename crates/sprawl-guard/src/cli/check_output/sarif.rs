use serde_json::{Value, json};
use sprawl_guard_lib::{
    CheckReport, CheckStatus, CheckWarning, RelativePath, UnreadablePathSeverity,
};

use super::artifact_uris::{ArtifactUris, ROOT_DIRECTORY};
use super::github::FilePaths;
use super::{gap_notices, warning_message};
use sprawl_guard_lib::check::{
    CheckViolation, DirectoryDirectFilesViolation, DirectoryViolationKind, FileBytesViolation,
    FileBytesViolationKind, FileLinesViolation, FileViolationKind, RatchetViolation,
    RatchetViolationKind,
};

const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const SARIF_VERSION: &str = "2.1.0";
const TOOL_NAME: &str = "sprawl-guard";

const FILE_LINES_RULE_ID: &str = "sprawl-guard/file-lines";
const TEST_FILE_LINES_RULE_ID: &str = "sprawl-guard/test-file-lines";
const DIRECTORY_DIRECT_FILES_RULE_ID: &str = "sprawl-guard/directory-direct-files";
const TEST_DIRECTORY_DIRECT_FILES_RULE_ID: &str = "sprawl-guard/test-directory-direct-files";
const FILE_BYTES_RULE_ID: &str = "sprawl-guard/file-bytes";
const LFS_OBJECT_BYTES_RULE_ID: &str = "sprawl-guard/lfs-object-bytes";
const RATCHET_STALE_RULE_ID: &str = "sprawl-guard/ratchet-stale";

pub(super) fn sarif_check_report(report: &CheckReport, file_paths: &FilePaths) -> Value {
    sarif_report(report, &ArtifactUris::new(file_paths))
}

fn sarif_report(report: &CheckReport, uris: &ArtifactUris) -> Value {
    json!({
        "$schema": SARIF_SCHEMA,
        "version": SARIF_VERSION,
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": TOOL_NAME,
                        "semanticVersion": env!("CARGO_PKG_VERSION"),
                        "rules": rules()
                    }
                },
                "invocations": [invocation(report, uris)],
                "results": results(report, uris)
            }
        ]
    })
}

/// Describes the run itself: whether it analyzed everything, and each gap and each skipped
/// selected path as a notification.
///
/// SARIF results are rule violations, so analysis gaps go here instead. A run whose gaps fail the
/// check is not an execution success.
fn invocation(report: &CheckReport, uris: &ArtifactUris) -> Value {
    let notifications = gap_notices(
        report,
        report.unreadable_paths().len(),
        report.uncounted_files().len(),
    )
    .map(|gap| {
        let level = match gap.severity {
            UnreadablePathSeverity::Error => "error",
            UnreadablePathSeverity::Warning => "warning",
        };
        let mut notification = json!({
            "level": level,
            "message": { "text": gap.message },
        });
        if let Some(file) = gap.file {
            notification["locations"] = json!(uris.file_locations(file));
        }
        notification
    })
    .chain(skipped_selection_notifications(report))
    .collect::<Vec<_>>();
    json!({
        "executionSuccessful": report.status() != CheckStatus::Error,
        "toolExecutionNotifications": notifications,
    })
}

/// Reports each selected path that the scan never reached, so a caller that reads only SARIF can
/// tell that a path it asked about went unchecked. Other warnings stay out of SARIF.
fn skipped_selection_notifications(report: &CheckReport) -> impl Iterator<Item = Value> {
    report
        .warnings()
        .iter()
        .filter(|warning| matches!(warning, CheckWarning::SelectedPathSkipped { .. }))
        .map(|warning| {
            json!({
                "level": "warning",
                "message": { "text": warning_message(warning) },
            })
        })
}

fn rules() -> Vec<Value> {
    vec![
        rule(
            FILE_LINES_RULE_ID,
            "File over the line limit",
            "A source file has more lines than its limit.",
        ),
        rule(
            TEST_FILE_LINES_RULE_ID,
            "Test file over the line limit",
            "A test file has more lines than its limit.",
        ),
        rule(
            DIRECTORY_DIRECT_FILES_RULE_ID,
            "Directory with too many direct files",
            "A directory directly contains more source files than its limit.",
        ),
        rule(
            TEST_DIRECTORY_DIRECT_FILES_RULE_ID,
            "Test directory with too many direct files",
            "A directory directly contains more test files than its limit.",
        ),
        rule(
            FILE_BYTES_RULE_ID,
            "File over the byte limit",
            "A file that Git stores, source or not, is larger than its byte limit.",
        ),
        rule(
            LFS_OBJECT_BYTES_RULE_ID,
            "Git LFS object over the byte limit",
            "A file that Git LFS stores is larger than its byte limit for Git LFS objects.",
        ),
        rule(
            RATCHET_STALE_RULE_ID,
            "Stale ratchet entry",
            "A ratchet entry's violation shrank or disappeared.",
        ),
    ]
}

fn rule(id: &str, short_description: &str, full_description: &str) -> Value {
    json!({
        "id": id,
        "shortDescription": {
            "text": short_description
        },
        "fullDescription": {
            "text": full_description
        }
    })
}

/// The rule a violation breaks and a one-sentence description of it. The SARIF and GitHub formats
/// share it so that both describe a violation in the same words.
pub(super) struct Finding {
    pub(super) rule_id: &'static str,
    pub(super) message: String,
}

/// Describes a violation in the words that every machine-readable format uses.
pub(super) fn finding(violation: &CheckViolation) -> Finding {
    match violation {
        CheckViolation::FileLines(violation) => file_lines_finding(violation),
        CheckViolation::DirectoryDirectFiles(violation) => {
            directory_direct_files_finding(violation)
        }
        CheckViolation::FileBytes(violation) => file_bytes_finding(violation),
        CheckViolation::Ratchet(violation) => Finding {
            rule_id: RATCHET_STALE_RULE_ID,
            message: ratchet_message(violation),
        },
    }
}

fn file_lines_finding(violation: &FileLinesViolation) -> Finding {
    Finding {
        rule_id: match violation.kind {
            FileViolationKind::FileLines => FILE_LINES_RULE_ID,
            FileViolationKind::TestFileLines => TEST_FILE_LINES_RULE_ID,
        },
        message: format!(
            "{} has {} lines (counting {}), over the limit of {}{}.",
            violation.path,
            violation.measurement.actual().get(),
            violation.measurement.count_lines(),
            violation.limit,
            ceiling_message(violation.ratchet)
        ),
    }
}

fn file_bytes_finding(violation: &FileBytesViolation) -> Finding {
    let (rule_id, subject) = match violation.kind {
        FileBytesViolationKind::FileBytes => {
            (FILE_BYTES_RULE_ID, format!("{} has", violation.path))
        }
        FileBytesViolationKind::LfsObjectBytes => (
            LFS_OBJECT_BYTES_RULE_ID,
            format!("{} is a Git LFS object of", violation.path),
        ),
    };
    Finding {
        rule_id,
        message: format!(
            "{subject} {} bytes, over the limit of {}{}.",
            violation.actual,
            violation.limit,
            ceiling_message(violation.ratchet)
        ),
    }
}

fn directory_direct_files_finding(violation: &DirectoryDirectFilesViolation) -> Finding {
    let (rule_id, source_kind) = match violation.kind {
        DirectoryViolationKind::DirectoryDirectFiles => {
            (DIRECTORY_DIRECT_FILES_RULE_ID, "production")
        }
        DirectoryViolationKind::TestDirectoryDirectFiles => {
            (TEST_DIRECTORY_DIRECT_FILES_RULE_ID, "test")
        }
    };
    Finding {
        rule_id,
        message: format!(
            "{} in group {} has {} direct {source_kind} files, over the limit of {}{}.",
            display_path(&violation.path),
            violation.group,
            violation.actual,
            violation.limit,
            ceiling_message(violation.ratchet)
        ),
    }
}

fn results(report: &CheckReport, uris: &ArtifactUris) -> Vec<Value> {
    report
        .violations()
        .iter()
        .map(|violation| match violation {
            CheckViolation::FileLines(violation) => file_lines_result(violation, uris),
            CheckViolation::DirectoryDirectFiles(violation) => {
                directory_direct_files_result(violation, uris)
            }
            CheckViolation::FileBytes(violation) => file_bytes_result(violation, uris),
            CheckViolation::Ratchet(violation) => ratchet_result(violation, uris),
        })
        .collect()
}

fn file_lines_result(violation: &FileLinesViolation, uris: &ArtifactUris) -> Value {
    let Finding { rule_id, message } = file_lines_finding(violation);
    json!({
        "ruleId": rule_id,
        "level": "error",
        "message": {
            "text": message
        },
        "locations": uris.file_locations(&violation.path),
        "properties": {
            "kind": violation.kind,
            "path": violation.path,
            "language": violation.language,
            "actual": violation.measurement.actual(),
            "count_lines": violation.measurement.count_lines(),
            "line_counts": violation.measurement.line_counts(),
            "limit": violation.limit,
            "ratchet": violation.ratchet
        }
    })
}

fn file_bytes_result(violation: &FileBytesViolation, uris: &ArtifactUris) -> Value {
    let Finding { rule_id, message } = file_bytes_finding(violation);
    json!({
        "ruleId": rule_id,
        "level": "error",
        "message": {
            "text": message
        },
        "locations": uris.file_locations(&violation.path),
        "properties": violation
    })
}

fn directory_direct_files_result(
    violation: &DirectoryDirectFilesViolation,
    uris: &ArtifactUris,
) -> Value {
    let Finding { rule_id, message } = directory_direct_files_finding(violation);
    let mut result = json!({
        "ruleId": rule_id,
        "level": "error",
        "message": {
            "text": message
        },
        "locations": uris.directory_locations(&violation.path),
        "properties": {
            "kind": violation.kind,
            "path": violation.path,
            "group": violation.group,
            "actual": violation.actual,
            "limit": violation.limit,
            "ratchet": violation.ratchet,
            "files": violation.files
        }
    });
    let related_locations = violation
        .files
        .iter()
        .filter_map(|path| uris.file_physical_location(path))
        .enumerate()
        .map(|(index, physical_location)| {
            json!({
                "id": index + 1,
                "physicalLocation": physical_location,
                "message": {
                    "text": "Direct file"
                }
            })
        })
        .collect::<Vec<_>>();
    if !related_locations.is_empty() {
        result["relatedLocations"] = Value::Array(related_locations);
    }
    result
}

/// Formats the ratchet ceiling that a violation grew above, if it has one.
fn ceiling_message(ceiling: Option<impl std::fmt::Display>) -> String {
    ceiling
        .map(|ceiling| format!(" and its ratchet ceiling of {ceiling}"))
        .unwrap_or_default()
}

fn ratchet_result(violation: &RatchetViolation, uris: &ArtifactUris) -> Value {
    let mut result = json!({
        "ruleId": RATCHET_STALE_RULE_ID,
        "level": "error",
        "message": {
            "text": ratchet_message(violation)
        },
        "locations": match violation.group() {
            Some(_) => uris.directory_locations(violation.path()),
            None => uris.file_locations(violation.path()),
        },
        "properties": {
            "kind": violation.kind(),
            "path": violation.path(),
            "actual": violation.actual(),
            "limit": violation.limit(),
            "ratchet": violation.ratchet()
        }
    });
    if let Some(group) = violation.group() {
        result["properties"]["group"] = json!(group);
    }
    if let Some(measurement) = violation.line_measurement() {
        result["properties"]["count_lines"] = json!(measurement.count_lines());
        result["properties"]["line_counts"] = json!(measurement.line_counts());
    }
    result
}

fn ratchet_message(violation: &RatchetViolation) -> String {
    let subject = match violation.group() {
        Some(group) => format!("{} in group {group}", display_path(violation.path())),
        None => display_path(violation.path()).to_owned(),
    };
    let measurement_context = match (violation.line_measurement(), violation.kind()) {
        (Some(measurement), _) => format!(" lines (counting {})", measurement.count_lines()),
        (None, RatchetViolationKind::FileBytesStale) => " bytes".to_owned(),
        (None, _) => String::new(),
    };
    match violation.actual() {
        Some(actual) => format!(
            "{subject} has a stale ratchet entry: ceiling {}, now {}{measurement_context}.",
            violation.ratchet().get(),
            actual.get()
        ),
        None => format!(
            "{subject} has a stale ratchet entry: ceiling {}, and it is no longer measured.",
            violation.ratchet().get()
        ),
    }
}

fn display_path(path: &RelativePath) -> &str {
    if path.as_str().is_empty() {
        ROOT_DIRECTORY
    } else {
        path.as_str()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;
    use sprawl_guard_lib::check::{
        CounterInfo, DirectoryDirectFilesViolation, DirectoryViolationKind, FileLinesViolation,
        FileRatchetStaleMissingViolation, FileViolationKind, RatchetStaleMeasuredViolation,
        RatchetStaleMissingViolation, RatchetViolation,
    };
    use sprawl_guard_lib::{
        CheckWarning, CheckedDirectoryCount, CheckedFileCount, DetectedLanguageCount,
        DiagnosticDetail, DirectFiles, DirectoryRatchetStaleMeasuredViolation,
        FileRatchetStaleMeasuredViolation, LanguageGroupId, LanguageId, LineCount, LineLimit,
        RatchetCount, RatchetEntryCount, RatchetLimit, RelativePath, ScanSummary,
    };

    use super::super::github::FilePaths;
    use super::*;

    fn scan_summary() -> ScanSummary {
        ScanSummary::new(
            CheckedFileCount::new(3),
            CheckedDirectoryCount::new(2),
            DetectedLanguageCount::new(1),
            RatchetEntryCount::new(0),
        )
    }

    /// Renders SARIF as a standalone report rooted at the checkout, whatever workspace the test
    /// process happens to run in.
    fn sarif_at_the_root(report: &CheckReport) -> Value {
        sarif_report(
            report,
            &ArtifactUris::new(&FilePaths::Prefixed(String::new())),
        )
    }

    pub(in crate::cli::check_output) fn report_with(
        violations: Vec<CheckViolation>,
    ) -> CheckReport {
        CheckReport::complete(
            sprawl_guard_lib::ReportRoot::trusted("/repo"),
            sprawl_guard_lib::ConfigSource::BuiltIn,
            CounterInfo {
                backend: "tokei",
                version: "14.0.0",
            },
            scan_summary(),
            vec![],
            violations,
        )
    }

    pub(in crate::cli::check_output) fn file_lines_violation(
        kind: FileViolationKind,
        path: &str,
    ) -> FileLinesViolation {
        FileLinesViolation {
            kind,
            path: RelativePath::new(path).unwrap(),
            language: LanguageId::new("Rust").unwrap(),
            measurement: line_measurement(3),
            limit: LineLimit::new(2).unwrap(),
            ratchet: None,
        }
    }

    fn line_measurement(actual: u32) -> sprawl_guard_lib::code_line_counter::LineMeasurement {
        sprawl_guard_lib::code_line_counter::LineMeasurement::new(
            sprawl_guard_lib::CountLines::Code,
            sprawl_guard_lib::LineCounts::new(
                LineCount::new(actual),
                LineCount::new(0),
                LineCount::new(0),
            ),
        )
        .unwrap()
    }

    pub(in crate::cli::check_output) fn directory_direct_files_violation(
        kind: DirectoryViolationKind,
        path: &str,
        files: Vec<&str>,
    ) -> DirectoryDirectFilesViolation {
        directory_direct_files_violation_in_group(kind, path, LanguageGroupId::Source, files)
    }

    fn directory_direct_files_violation_in_group(
        kind: DirectoryViolationKind,
        path: &str,
        group: LanguageGroupId,
        files: Vec<&str>,
    ) -> DirectoryDirectFilesViolation {
        DirectoryDirectFilesViolation {
            kind,
            group,
            path: RelativePath::new(path).unwrap(),
            actual: DirectFiles::new(2).unwrap(),
            limit: DirectFiles::new(1).unwrap(),
            ratchet: None,
            files: files
                .into_iter()
                .map(|path| RelativePath::new(path).unwrap())
                .collect(),
        }
    }

    mod when_rendering_the_schema {
        use super::*;

        #[test]
        fn it_uses_sarif_2_1_0_with_stable_rule_ids() {
            let output = sarif_at_the_root(&report_with(vec![]));

            assert_eq!(
                json!({
                    "version": output["version"],
                    "driver": output["runs"][0]["tool"]["driver"],
                }),
                json!({
                    "version": "2.1.0",
                    "driver": {
                        "name": "sprawl-guard",
                        "semanticVersion": env!("CARGO_PKG_VERSION"),
                        "rules": [
                            {
                                "id": "sprawl-guard/file-lines",
                                "shortDescription": {
                                    "text": "File over the line limit"
                                },
                                "fullDescription": {
                                    "text": "A source file has more lines than its limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/test-file-lines",
                                "shortDescription": {
                                    "text": "Test file over the line limit"
                                },
                                "fullDescription": {
                                    "text": "A test file has more lines than its limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/directory-direct-files",
                                "shortDescription": {
                                    "text": "Directory with too many direct files"
                                },
                                "fullDescription": {
                                    "text": "A directory directly contains more source files than its limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/test-directory-direct-files",
                                "shortDescription": {
                                    "text": "Test directory with too many direct files"
                                },
                                "fullDescription": {
                                    "text": "A directory directly contains more test files than its limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/file-bytes",
                                "shortDescription": {
                                    "text": "File over the byte limit"
                                },
                                "fullDescription": {
                                    "text": "A file that Git stores, source or not, is larger than its byte limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/lfs-object-bytes",
                                "shortDescription": {
                                    "text": "Git LFS object over the byte limit"
                                },
                                "fullDescription": {
                                    "text": "A file that Git LFS stores is larger than its byte limit for Git LFS objects."
                                }
                            },
                            {
                                "id": "sprawl-guard/ratchet-stale",
                                "shortDescription": {
                                    "text": "Stale ratchet entry"
                                },
                                "fullDescription": {
                                    "text": "A ratchet entry's violation shrank or disappeared."
                                }
                            }
                        ]
                    }
                })
            );
        }

        #[test]
        fn it_omits_non_fatal_check_warnings() {
            let report = CheckReport::complete(
                sprawl_guard_lib::ReportRoot::trusted("/repo"),
                sprawl_guard_lib::ConfigSource::BuiltIn,
                CounterInfo {
                    backend: "tokei",
                    version: "14.0.0",
                },
                scan_summary(),
                vec![CheckWarning::RustCfgTestParseFailed {
                    path: RelativePath::new("src/lib.rs").unwrap(),
                    detail: DiagnosticDetail::new("expected identifier"),
                }],
                vec![],
            );

            let output = sarif_at_the_root(&report);

            assert_eq!(
                json!({
                    "results": output["runs"][0]["results"],
                    "warnings": output["runs"][0].get("warnings"),
                }),
                json!({
                    "results": [],
                    "warnings": null,
                })
            );
        }
    }

    mod when_the_scan_skipped_a_selected_path {
        use super::*;

        #[test]
        fn it_reports_the_path_as_a_warning_notification() {
            let report = CheckReport::complete(
                sprawl_guard_lib::ReportRoot::trusted("/repo"),
                sprawl_guard_lib::ConfigSource::BuiltIn,
                CounterInfo {
                    backend: "tokei",
                    version: "14.0.0",
                },
                scan_summary(),
                vec![CheckWarning::SelectedPathSkipped {
                    path: RelativePath::new(".hidden.rs").unwrap(),
                }],
                vec![],
            );

            let output = sarif_at_the_root(&report);

            assert_eq!(
                output["runs"][0]["invocations"][0]["toolExecutionNotifications"],
                json!([{
                    "level": "warning",
                    "message": {
                        "text": ".hidden.rs: not checked, because the scan did not reach it; the scan skips hidden and ignored paths",
                    },
                }])
            );
        }
    }

    mod when_rendering_file_lines_findings {
        use super::*;

        #[test]
        fn it_maps_the_file_to_a_sarif_location() {
            let report = report_with(vec![CheckViolation::FileLines(file_lines_violation(
                FileViolationKind::FileLines,
                "src/lib.rs",
            ))]);

            let output = sarif_at_the_root(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                json!({
                    "ruleId": result["ruleId"],
                    "message": result["message"]["text"],
                    "location": result["locations"][0]["physicalLocation"],
                    "properties": {
                        "language": result["properties"]["language"],
                        "actual": result["properties"]["actual"],
                    },
                }),
                json!({
                    "ruleId": FILE_LINES_RULE_ID,
                    "message": "src/lib.rs has 3 lines (counting code), over the limit of 2.",
                    "location": {
                        "artifactLocation": {
                            "uri": "src/lib.rs",
                        },
                        "region": {
                            "startLine": 1,
                        },
                    },
                    "properties": {
                        "language": "Rust",
                        "actual": 3,
                    },
                })
            );
        }

        #[test]
        fn it_names_the_ceiling_of_a_file_that_grew_above_it() {
            let report = report_with(vec![CheckViolation::FileLines(FileLinesViolation {
                limit: LineLimit::new(1).unwrap(),
                ratchet: Some(LineLimit::new(2).unwrap()),
                ..file_lines_violation(FileViolationKind::FileLines, "src/lib.rs")
            })]);

            let output = sarif_at_the_root(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                json!({
                    "ruleId": result["ruleId"],
                    "message": result["message"]["text"],
                    "ratchet": result["properties"]["ratchet"],
                }),
                json!({
                    "ruleId": FILE_LINES_RULE_ID,
                    "message": "src/lib.rs has 3 lines (counting code), over the limit of 1 and its ratchet ceiling of 2.",
                    "ratchet": 2,
                })
            );
        }

        #[test]
        fn it_uses_the_test_file_rule_for_test_file_violations() {
            let report = report_with(vec![CheckViolation::FileLines(file_lines_violation(
                FileViolationKind::TestFileLines,
                "src/lib.test.rs",
            ))]);

            let output = sarif_at_the_root(&report);

            assert_eq!(
                output["runs"][0]["results"][0]["ruleId"],
                TEST_FILE_LINES_RULE_ID
            );
        }
    }

    mod when_naming_files {
        use super::*;

        fn first_uri(path: &str, file_paths: &FilePaths) -> Value {
            let report = report_with(vec![CheckViolation::FileLines(file_lines_violation(
                FileViolationKind::FileLines,
                path,
            ))]);
            let output = sarif_report(&report, &ArtifactUris::new(file_paths));
            output["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
                ["uri"]
                .clone()
        }

        fn at_the_workspace_root() -> FilePaths {
            FilePaths::Prefixed(String::new())
        }

        #[test]
        fn it_percent_encodes_characters_that_a_uri_reserves() {
            assert_eq!(
                first_uri("src/my dir/a#1 100%.rs", &at_the_workspace_root()),
                "src/my%20dir/a%231%20100%25.rs"
            );
        }

        #[test]
        fn it_percent_encodes_non_ascii_text_as_utf_8() {
            assert_eq!(
                first_uri("src/héllo?.rs", &at_the_workspace_root()),
                "src/h%C3%A9llo%3F.rs"
            );
        }

        #[test]
        fn it_prefixes_the_place_of_the_root_in_the_workspace() {
            assert_eq!(
                first_uri(
                    "src/lib.rs",
                    &FilePaths::Prefixed("packages/my app/".to_owned())
                ),
                "packages/my%20app/src/lib.rs"
            );
        }

        #[test]
        fn it_gives_no_location_under_a_root_outside_the_workspace() {
            let report = report_with(vec![CheckViolation::FileLines(file_lines_violation(
                FileViolationKind::FileLines,
                "src/lib.rs",
            ))]);

            let output = sarif_report(&report, &ArtifactUris::new(&FilePaths::OutsideWorkspace));
            let result = &output["runs"][0]["results"][0];

            assert_eq!(result["locations"], json!([]));
            assert_eq!(
                result["message"]["text"],
                "src/lib.rs has 3 lines (counting code), over the limit of 2."
            );
        }

        #[test]
        fn it_names_the_root_directory_with_a_dot() {
            let report = report_with(vec![CheckViolation::DirectoryDirectFiles(
                directory_direct_files_violation(
                    DirectoryViolationKind::DirectoryDirectFiles,
                    "",
                    vec!["a.rs", "b.rs"],
                ),
            )]);

            let output = sarif_report(&report, &ArtifactUris::new(&at_the_workspace_root()));

            assert_eq!(
                output["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
                    ["uri"],
                "."
            );
        }
    }

    mod when_rendering_directory_findings {
        use super::*;

        #[test]
        fn it_maps_child_files_to_related_locations() {
            let report = report_with(vec![CheckViolation::DirectoryDirectFiles(
                directory_direct_files_violation(
                    DirectoryViolationKind::DirectoryDirectFiles,
                    "src",
                    vec!["src/a.rs", "src/b.rs"],
                ),
            )]);

            let output = sarif_at_the_root(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                json!({
                    "ruleId": result["ruleId"],
                    "path": result["properties"]["path"],
                    "relatedLocations": [
                        result["relatedLocations"][0]["physicalLocation"]["artifactLocation"],
                        result["relatedLocations"][1]["physicalLocation"]["artifactLocation"],
                    ],
                }),
                json!({
                    "ruleId": DIRECTORY_DIRECT_FILES_RULE_ID,
                    "path": "src",
                    "relatedLocations": [
                        {"uri": "src/a.rs"},
                        {"uri": "src/b.rs"},
                    ],
                })
            );
        }

        #[test]
        fn it_gives_the_directory_no_line_and_each_file_its_first_line() {
            let report = report_with(vec![CheckViolation::DirectoryDirectFiles(
                directory_direct_files_violation(
                    DirectoryViolationKind::DirectoryDirectFiles,
                    "src",
                    vec!["src/a.rs"],
                ),
            )]);

            let output = sarif_at_the_root(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                json!({
                    "directory": result["locations"][0]["physicalLocation"],
                    "file": result["relatedLocations"][0]["physicalLocation"],
                }),
                json!({
                    "directory": { "artifactLocation": { "uri": "src" } },
                    "file": {
                        "artifactLocation": { "uri": "src/a.rs" },
                        "region": { "startLine": 1 },
                    },
                })
            );
        }

        #[test]
        fn it_uses_the_test_directory_rule_for_test_directory_violations() {
            let report = report_with(vec![CheckViolation::DirectoryDirectFiles(
                directory_direct_files_violation(
                    DirectoryViolationKind::TestDirectoryDirectFiles,
                    "src",
                    vec!["src/lib.test.rs"],
                ),
            )]);

            let output = sarif_at_the_root(&report);

            assert_eq!(
                output["runs"][0]["results"][0]["ruleId"],
                TEST_DIRECTORY_DIRECT_FILES_RULE_ID
            );
            assert_eq!(
                output["runs"][0]["results"][0]["properties"]["group"],
                "source"
            );
        }

        #[test]
        fn it_names_the_ceiling_of_a_directory_that_grew_above_it() {
            let report = report_with(vec![CheckViolation::DirectoryDirectFiles(
                DirectoryDirectFilesViolation {
                    actual: DirectFiles::new(3).unwrap(),
                    ratchet: Some(DirectFiles::new(2).unwrap()),
                    ..directory_direct_files_violation(
                        DirectoryViolationKind::DirectoryDirectFiles,
                        "src",
                        vec!["src/a.rs", "src/b.rs", "src/c.rs"],
                    )
                },
            )]);

            let output = sarif_at_the_root(&report);

            assert_eq!(
                output["runs"][0]["results"][0]["message"]["text"],
                "src in group source has 3 direct production files, over the limit of 1 and its ratchet ceiling of 2."
            );
        }

        #[test]
        fn it_identifies_groups_when_directory_findings_share_a_path() {
            let report = report_with(vec![
                CheckViolation::DirectoryDirectFiles(directory_direct_files_violation_in_group(
                    DirectoryViolationKind::DirectoryDirectFiles,
                    "src",
                    LanguageGroupId::Source,
                    vec!["src/a.rs", "src/b.rs"],
                )),
                CheckViolation::DirectoryDirectFiles(directory_direct_files_violation_in_group(
                    DirectoryViolationKind::DirectoryDirectFiles,
                    "src",
                    LanguageGroupId::Documentation,
                    vec!["src/a.md", "src/b.md"],
                )),
            ]);

            let output = sarif_at_the_root(&report);
            let results = &output["runs"][0]["results"];

            assert_eq!(
                results[0]["message"]["text"],
                "src in group source has 2 direct production files, over the limit of 1."
            );
            assert_eq!(
                results[1]["message"]["text"],
                "src in group documentation has 2 direct production files, over the limit of 1."
            );
        }
    }

    mod when_rendering_multiple_findings {
        use super::*;

        #[test]
        fn it_preserves_the_check_report_order() {
            let report = report_with(vec![
                CheckViolation::DirectoryDirectFiles(directory_direct_files_violation(
                    DirectoryViolationKind::DirectoryDirectFiles,
                    "src",
                    vec!["src/a.rs"],
                )),
                CheckViolation::FileLines(file_lines_violation(
                    FileViolationKind::FileLines,
                    "src/lib.rs",
                )),
            ]);

            let output = sarif_at_the_root(&report);
            let results = &output["runs"][0]["results"];

            assert_eq!(results[0]["ruleId"], DIRECTORY_DIRECT_FILES_RULE_ID);
            assert_eq!(results[1]["ruleId"], FILE_LINES_RULE_ID);
        }
    }

    mod when_rendering_ratchet_findings {
        use super::*;

        #[test]
        fn it_maps_stale_entries_to_the_ratchet_stale_rule() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Measured(
                RatchetStaleMeasuredViolation::Directory(DirectoryRatchetStaleMeasuredViolation {
                    kind: DirectoryViolationKind::DirectoryDirectFiles,
                    path: RelativePath::new("src").unwrap(),
                    group: LanguageGroupId::Source,
                    actual: RatchetCount::new(1),
                    limit: RatchetLimit::Finite(RatchetCount::new(2)),
                    ratchet: RatchetCount::new(4),
                }),
            ))]);

            let output = sarif_at_the_root(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(result["ruleId"], RATCHET_STALE_RULE_ID);
            assert_eq!(result["properties"]["kind"], "directory_direct_files_stale");
            assert_eq!(result["properties"]["group"], "source");
        }

        #[test]
        fn it_identifies_count_lines_for_measured_file_staleness() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Measured(
                RatchetStaleMeasuredViolation::File(FileRatchetStaleMeasuredViolation {
                    kind: FileViolationKind::FileLines,
                    path: RelativePath::new("src/lib.rs").unwrap(),
                    measurement: line_measurement(2),
                    limit: RatchetCount::new(3),
                    ratchet: RatchetCount::new(4),
                }),
            ))]);

            let output = sarif_at_the_root(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                result["message"]["text"],
                "src/lib.rs has a stale ratchet entry: ceiling 4, now 2 lines (counting code)."
            );
        }

        #[test]
        fn it_reports_deleted_ratchet_entries_as_no_longer_measured() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Missing(
                RatchetStaleMissingViolation::File(FileRatchetStaleMissingViolation {
                    kind: FileViolationKind::FileLines,
                    path: RelativePath::new("src/deleted.rs").unwrap(),
                    ratchet: RatchetCount::new(4),
                }),
            ))]);

            let output = sarif_at_the_root(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                json!({
                    "ruleId": result["ruleId"],
                    "actual": result["properties"]["actual"],
                    "message": result["message"]["text"],
                }),
                json!({
                    "ruleId": RATCHET_STALE_RULE_ID,
                    "actual": null,
                    "message": "src/deleted.rs has a stale ratchet entry: ceiling 4, and it is no longer measured.",
                })
            );
        }

        #[test]
        fn it_distinguishes_missing_directory_ratchets_for_groups_at_the_same_path() {
            let report = report_with(vec![
                crate::cli::check_output::missing_directory_ratchet(LanguageGroupId::Source, 4),
                crate::cli::check_output::missing_directory_ratchet(
                    LanguageGroupId::Documentation,
                    5,
                ),
            ]);

            let output = sarif_at_the_root(&report);
            let results = &output["runs"][0]["results"];

            assert_eq!(
                json!([
                    {
                        "group": results[0]["properties"]["group"],
                        "message": results[0]["message"]["text"],
                    },
                    {
                        "group": results[1]["properties"]["group"],
                        "message": results[1]["message"]["text"],
                    },
                ]),
                json!([
                    {
                        "group": "source",
                        "message": "src in group source has a stale ratchet entry: ceiling 4, and it is no longer measured.",
                    },
                    {
                        "group": "documentation",
                        "message": "src in group documentation has a stale ratchet entry: ceiling 5, and it is no longer measured.",
                    },
                ])
            );
        }
    }
}
