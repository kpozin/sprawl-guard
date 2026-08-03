use serde_json::{Value, json};
use sprawl_guard_lib::CheckReport;
use sprawl_guard_lib::RelativePath;
use sprawl_guard_lib::check::{
    CheckViolation, DirectoryFanoutViolation, DirectoryViolationKind, FileLocViolation,
    FileViolationKind, RatchetViolation, RatchetViolationKind,
};

const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const SARIF_VERSION: &str = "2.1.0";
const TOOL_NAME: &str = "sprawl-guard";

const FILE_LOC_RULE_ID: &str = "sprawl-guard/file-loc";
const TEST_FILE_LOC_RULE_ID: &str = "sprawl-guard/test-file-loc";
const DIRECTORY_FANOUT_RULE_ID: &str = "sprawl-guard/directory-fanout";
const TEST_DIRECTORY_FANOUT_RULE_ID: &str = "sprawl-guard/test-directory-fanout";
const RATCHET_INCREASE_RULE_ID: &str = "sprawl-guard/ratchet-increase";
const RATCHET_STALE_RULE_ID: &str = "sprawl-guard/ratchet-stale";

pub(super) fn sarif_check_report(report: &CheckReport) -> Value {
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
                "results": results(report)
            }
        ]
    })
}

fn rules() -> Vec<Value> {
    vec![
        rule(
            FILE_LOC_RULE_ID,
            "File LOC budget exceeded",
            "A production source file exceeded its resolved configured-LOC limit.",
        ),
        rule(
            TEST_FILE_LOC_RULE_ID,
            "Test file LOC budget exceeded",
            "A path-classified test file exceeded its resolved configured-LOC limit.",
        ),
        rule(
            DIRECTORY_FANOUT_RULE_ID,
            "Directory fan-out budget exceeded",
            "A production directory exceeded its resolved direct leaf-file limit.",
        ),
        rule(
            TEST_DIRECTORY_FANOUT_RULE_ID,
            "Test directory fan-out budget exceeded",
            "A test directory exceeded its resolved direct leaf-file limit.",
        ),
        rule(
            RATCHET_INCREASE_RULE_ID,
            "Ratchet ceiling exceeded",
            "A measured value increased above its stored ratchet ceiling.",
        ),
        rule(
            RATCHET_STALE_RULE_ID,
            "Ratchet entry is stale",
            "A stored ratchet ceiling is no longer current for the measured value.",
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

fn results(report: &CheckReport) -> Vec<Value> {
    report
        .violations()
        .iter()
        .map(|violation| match violation {
            CheckViolation::FileLoc(violation) => file_loc_result(violation),
            CheckViolation::DirectoryFanout(violation) => directory_fanout_result(violation),
            CheckViolation::Ratchet(violation) => ratchet_result(violation),
        })
        .collect()
}

fn file_loc_result(violation: &FileLocViolation) -> Value {
    let rule_id = match violation.kind {
        FileViolationKind::FileLoc => FILE_LOC_RULE_ID,
        FileViolationKind::TestFileLoc => TEST_FILE_LOC_RULE_ID,
    };
    json!({
        "ruleId": rule_id,
        "level": "error",
        "message": {
            "text": format!(
                "{} has {} configured LOC (included line types: {}), exceeding limit {}.",
                violation.path,
                violation.measurement.actual().get(),
                violation.measurement.included_line_types(),
                violation.limit
            )
        },
        "locations": [location(&violation.path)],
        "properties": {
            "kind": violation.kind,
            "path": violation.path,
            "language": violation.language,
            "actual": violation.measurement.actual(),
            "included_line_types": violation.measurement.included_line_types(),
            "line_counts": violation.measurement.line_counts(),
            "limit": violation.limit,
            "ratchet": violation.ratchet
        }
    })
}

fn directory_fanout_result(violation: &DirectoryFanoutViolation) -> Value {
    let (rule_id, source_kind) = match violation.kind {
        DirectoryViolationKind::DirectoryFanout => (DIRECTORY_FANOUT_RULE_ID, "production"),
        DirectoryViolationKind::TestDirectoryFanout => (TEST_DIRECTORY_FANOUT_RULE_ID, "test"),
    };
    let mut result = json!({
        "ruleId": rule_id,
        "level": "error",
        "message": {
            "text": format!(
                "{} in group {} has {} direct {source_kind} leaf files, exceeding limit {}.",
                display_path(&violation.path),
                violation.group,
                violation.actual,
                violation.limit
            )
        },
        "locations": [location(&violation.path)],
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
    if !violation.files.is_empty() {
        result["relatedLocations"] = Value::Array(
            violation
                .files
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    json!({
                        "id": index + 1,
                        "physicalLocation": physical_location(path),
                        "message": {
                            "text": "Child leaf file"
                        }
                    })
                })
                .collect(),
        );
    }
    result
}

fn ratchet_result(violation: &RatchetViolation) -> Value {
    let rule_id = match violation.kind() {
        RatchetViolationKind::FileLocIncreased
        | RatchetViolationKind::TestFileLocIncreased
        | RatchetViolationKind::DirectoryFanoutIncreased
        | RatchetViolationKind::TestDirectoryFanoutIncreased => RATCHET_INCREASE_RULE_ID,
        RatchetViolationKind::FileLocStale
        | RatchetViolationKind::TestFileLocStale
        | RatchetViolationKind::DirectoryFanoutStale
        | RatchetViolationKind::TestDirectoryFanoutStale => RATCHET_STALE_RULE_ID,
    };
    let mut result = json!({
        "ruleId": rule_id,
        "level": "error",
        "message": {
            "text": ratchet_message(violation)
        },
        "locations": [location(violation.path())],
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
    if let Some(measurement) = violation.loc_measurement() {
        result["properties"]["included_line_types"] = json!(measurement.included_line_types());
        result["properties"]["line_counts"] = json!(measurement.line_counts());
    }
    result
}

fn ratchet_message(violation: &RatchetViolation) -> String {
    let subject = match violation.group() {
        Some(group) => format!("{} in group {group}", display_path(violation.path())),
        None => display_path(violation.path()).to_owned(),
    };
    let measurement_context = violation
        .loc_measurement()
        .map_or_else(String::new, |measurement| {
            format!(
                " configured LOC (included line types: {})",
                measurement.included_line_types()
            )
        });
    match violation.kind() {
        RatchetViolationKind::FileLocIncreased
        | RatchetViolationKind::TestFileLocIncreased
        | RatchetViolationKind::DirectoryFanoutIncreased
        | RatchetViolationKind::TestDirectoryFanoutIncreased => {
            let actual = violation
                .actual()
                .map(|value| value.get().to_string())
                .unwrap_or_else(|| "not measured".to_owned());
            format!(
                "{subject} increased to {actual}{measurement_context} above ratchet ceiling {}.",
                violation.ratchet().get()
            )
        }
        RatchetViolationKind::FileLocStale
        | RatchetViolationKind::TestFileLocStale
        | RatchetViolationKind::DirectoryFanoutStale
        | RatchetViolationKind::TestDirectoryFanoutStale => match violation.actual() {
            Some(actual) => format!(
                "{subject} has stale ratchet ceiling {}; current value is {}{measurement_context}.",
                violation.ratchet().get(),
                actual.get()
            ),
            None => format!(
                "{subject} has stale ratchet ceiling {} but is no longer measured.",
                violation.ratchet().get()
            ),
        },
    }
}

fn location(path: &RelativePath) -> Value {
    json!({
        "physicalLocation": physical_location(path)
    })
}

fn physical_location(path: &RelativePath) -> Value {
    json!({
        "artifactLocation": {
            "uri": uri(path)
        },
        "region": {
            "startLine": 1
        }
    })
}

fn uri(path: &RelativePath) -> &str {
    if path.as_str().is_empty() {
        "."
    } else {
        path.as_str()
    }
}

fn display_path(path: &RelativePath) -> &str {
    uri(path)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sprawl_guard_lib::check::{
        CounterInfo, DirectoryFanoutViolation, DirectoryViolationKind, FileLocViolation,
        FileRatchetStaleMissingViolation, FileViolationKind, RatchetIncreasedViolation,
        RatchetStaleMeasuredViolation, RatchetStaleMissingViolation, RatchetStaleViolation,
        RatchetStaleViolationKind, RatchetViolation,
    };
    use sprawl_guard_lib::{
        CheckWarning, CheckWarningKind, DiagnosticDetail, DirectoryRatchetStaleMeasuredViolation,
        FileRatchetIncreasedViolation, FileRatchetStaleMeasuredViolation, LanguageGroupId,
        LanguageId, LeafFiles, LineCount, LocLimit, RatchetCount, RatchetLimit, RelativePath,
    };

    use super::*;

    fn report_with(violations: Vec<CheckViolation>) -> CheckReport {
        CheckReport::complete(
            sprawl_guard_lib::ReportRoot::trusted("/repo"),
            CounterInfo {
                backend: "tokei",
                version: "14.0.0",
            },
            vec![],
            violations,
        )
    }

    fn file_loc_violation(kind: FileViolationKind, path: &str) -> FileLocViolation {
        FileLocViolation {
            kind,
            path: RelativePath::new(path).unwrap(),
            language: LanguageId::new("Rust").unwrap(),
            measurement: loc_measurement(3),
            limit: LocLimit::new(2).unwrap(),
            ratchet: None,
        }
    }

    fn loc_measurement(actual: u32) -> sprawl_guard_lib::code_line_counter::LocMeasurement {
        sprawl_guard_lib::code_line_counter::LocMeasurement::new(
            sprawl_guard_lib::IncludedLineTypes::Code,
            sprawl_guard_lib::LineCounts::new(
                LineCount::new(actual),
                LineCount::new(0),
                LineCount::new(0),
            ),
        )
        .unwrap()
    }

    fn directory_fanout_violation(
        kind: DirectoryViolationKind,
        path: &str,
        files: Vec<&str>,
    ) -> DirectoryFanoutViolation {
        directory_fanout_violation_in_group(kind, path, LanguageGroupId::Source, files)
    }

    fn directory_fanout_violation_in_group(
        kind: DirectoryViolationKind,
        path: &str,
        group: LanguageGroupId,
        files: Vec<&str>,
    ) -> DirectoryFanoutViolation {
        DirectoryFanoutViolation {
            kind,
            group,
            path: RelativePath::new(path).unwrap(),
            actual: LeafFiles::new(2).unwrap(),
            limit: LeafFiles::new(1).unwrap(),
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
            let output = sarif_check_report(&report_with(vec![]));

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
                                "id": "sprawl-guard/file-loc",
                                "shortDescription": {
                                    "text": "File LOC budget exceeded"
                                },
                                "fullDescription": {
                                    "text": "A production source file exceeded its resolved configured-LOC limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/test-file-loc",
                                "shortDescription": {
                                    "text": "Test file LOC budget exceeded"
                                },
                                "fullDescription": {
                                    "text": "A path-classified test file exceeded its resolved configured-LOC limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/directory-fanout",
                                "shortDescription": {
                                    "text": "Directory fan-out budget exceeded"
                                },
                                "fullDescription": {
                                    "text": "A production directory exceeded its resolved direct leaf-file limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/test-directory-fanout",
                                "shortDescription": {
                                    "text": "Test directory fan-out budget exceeded"
                                },
                                "fullDescription": {
                                    "text": "A test directory exceeded its resolved direct leaf-file limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/ratchet-increase",
                                "shortDescription": {
                                    "text": "Ratchet ceiling exceeded"
                                },
                                "fullDescription": {
                                    "text": "A measured value increased above its stored ratchet ceiling."
                                }
                            },
                            {
                                "id": "sprawl-guard/ratchet-stale",
                                "shortDescription": {
                                    "text": "Ratchet entry is stale"
                                },
                                "fullDescription": {
                                    "text": "A stored ratchet ceiling is no longer current for the measured value."
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
                CounterInfo {
                    backend: "tokei",
                    version: "14.0.0",
                },
                vec![CheckWarning {
                    kind: CheckWarningKind::RustCfgTestParseFailed,
                    path: RelativePath::new("src/lib.rs").unwrap(),
                    detail: DiagnosticDetail::new("expected identifier"),
                }],
                vec![],
            );

            let output = sarif_check_report(&report);

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

    mod when_rendering_file_loc_findings {
        use super::*;

        #[test]
        fn it_maps_the_file_to_a_sarif_location() {
            let report = report_with(vec![CheckViolation::FileLoc(file_loc_violation(
                FileViolationKind::FileLoc,
                "src/lib.rs",
            ))]);

            let output = sarif_check_report(&report);
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
                    "ruleId": FILE_LOC_RULE_ID,
                    "message": "src/lib.rs has 3 configured LOC (included line types: code), exceeding limit 2.",
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
        fn it_uses_the_test_file_rule_for_test_file_violations() {
            let report = report_with(vec![CheckViolation::FileLoc(file_loc_violation(
                FileViolationKind::TestFileLoc,
                "src/lib.test.rs",
            ))]);

            let output = sarif_check_report(&report);

            assert_eq!(
                output["runs"][0]["results"][0]["ruleId"],
                TEST_FILE_LOC_RULE_ID
            );
        }
    }

    mod when_rendering_directory_findings {
        use super::*;

        #[test]
        fn it_maps_child_files_to_related_locations() {
            let report = report_with(vec![CheckViolation::DirectoryFanout(
                directory_fanout_violation(
                    DirectoryViolationKind::DirectoryFanout,
                    "src",
                    vec!["src/a.rs", "src/b.rs"],
                ),
            )]);

            let output = sarif_check_report(&report);
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
                    "ruleId": DIRECTORY_FANOUT_RULE_ID,
                    "path": "src",
                    "relatedLocations": [
                        {"uri": "src/a.rs"},
                        {"uri": "src/b.rs"},
                    ],
                })
            );
        }

        #[test]
        fn it_uses_the_test_directory_rule_for_test_directory_violations() {
            let report = report_with(vec![CheckViolation::DirectoryFanout(
                directory_fanout_violation(
                    DirectoryViolationKind::TestDirectoryFanout,
                    "src",
                    vec!["src/lib.test.rs"],
                ),
            )]);

            let output = sarif_check_report(&report);

            assert_eq!(
                output["runs"][0]["results"][0]["ruleId"],
                TEST_DIRECTORY_FANOUT_RULE_ID
            );
            assert_eq!(
                output["runs"][0]["results"][0]["properties"]["group"],
                "source"
            );
        }

        #[test]
        fn it_identifies_groups_when_directory_findings_share_a_path() {
            let report = report_with(vec![
                CheckViolation::DirectoryFanout(directory_fanout_violation_in_group(
                    DirectoryViolationKind::DirectoryFanout,
                    "src",
                    LanguageGroupId::Source,
                    vec!["src/a.rs", "src/b.rs"],
                )),
                CheckViolation::DirectoryFanout(directory_fanout_violation_in_group(
                    DirectoryViolationKind::DirectoryFanout,
                    "src",
                    LanguageGroupId::Documentation,
                    vec!["src/a.md", "src/b.md"],
                )),
            ]);

            let output = sarif_check_report(&report);
            let results = &output["runs"][0]["results"];

            assert_eq!(
                results[0]["message"]["text"],
                "src in group source has 2 direct production leaf files, exceeding limit 1."
            );
            assert_eq!(
                results[1]["message"]["text"],
                "src in group documentation has 2 direct production leaf files, exceeding limit 1."
            );
        }
    }

    mod when_rendering_multiple_findings {
        use super::*;

        #[test]
        fn it_preserves_the_check_report_order() {
            let report = report_with(vec![
                CheckViolation::DirectoryFanout(directory_fanout_violation(
                    DirectoryViolationKind::DirectoryFanout,
                    "src",
                    vec!["src/a.rs"],
                )),
                CheckViolation::FileLoc(file_loc_violation(
                    FileViolationKind::FileLoc,
                    "src/lib.rs",
                )),
            ]);

            let output = sarif_check_report(&report);
            let results = &output["runs"][0]["results"];

            assert_eq!(results[0]["ruleId"], DIRECTORY_FANOUT_RULE_ID);
            assert_eq!(results[1]["ruleId"], FILE_LOC_RULE_ID);
        }
    }

    mod when_rendering_ratchet_findings {
        use super::*;

        #[test]
        fn it_maps_increases_to_the_ratchet_increase_rule() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Increased(
                RatchetIncreasedViolation::File(FileRatchetIncreasedViolation {
                    kind: FileViolationKind::FileLoc,
                    path: RelativePath::new("src/lib.rs").unwrap(),
                    measurement: loc_measurement(5),
                    limit: RatchetCount::new(2),
                    ratchet: RatchetCount::new(4),
                }),
            ))]);

            let output = sarif_check_report(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                json!({
                    "ruleId": result["ruleId"],
                    "message": result["message"]["text"],
                    "properties": {
                        "actual": result["properties"]["actual"],
                        "included_line_types": result["properties"]["included_line_types"],
                        "line_counts": result["properties"]["line_counts"],
                        "limit": result["properties"]["limit"],
                        "ratchet": result["properties"]["ratchet"],
                    },
                }),
                json!({
                    "ruleId": RATCHET_INCREASE_RULE_ID,
                    "message": "src/lib.rs increased to 5 configured LOC (included line types: code) above ratchet ceiling 4.",
                    "properties": {
                        "actual": 5,
                        "included_line_types": ["code"],
                        "line_counts": {"code": 5, "comments": 0, "blank_lines": 0},
                        "limit": 2,
                        "ratchet": 4,
                    },
                })
            );
            assert!(result["properties"].get("group").is_none());
        }

        #[test]
        fn it_maps_stale_entries_to_the_ratchet_stale_rule() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Stale(
                RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::Directory(
                    DirectoryRatchetStaleMeasuredViolation {
                        kind: DirectoryViolationKind::DirectoryFanout,
                        path: RelativePath::new("src").unwrap(),
                        group: LanguageGroupId::Source,
                        actual: RatchetCount::new(1),
                        limit: RatchetLimit::Finite(RatchetCount::new(2)),
                        ratchet: RatchetCount::new(4),
                    },
                )),
            ))]);

            let output = sarif_check_report(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(result["ruleId"], RATCHET_STALE_RULE_ID);
            assert_eq!(result["properties"]["kind"], "directory_fanout_stale");
            assert_eq!(result["properties"]["group"], "source");
        }

        #[test]
        fn it_identifies_included_line_types_for_measured_file_staleness() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Stale(
                RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation::File(
                    FileRatchetStaleMeasuredViolation {
                        kind: FileViolationKind::FileLoc,
                        path: RelativePath::new("src/lib.rs").unwrap(),
                        measurement: loc_measurement(2),
                        limit: RatchetCount::new(3),
                        ratchet: RatchetCount::new(4),
                    },
                )),
            ))]);

            let output = sarif_check_report(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                result["message"]["text"],
                "src/lib.rs has stale ratchet ceiling 4; current value is 2 configured LOC (included line types: code)."
            );
        }

        #[test]
        fn it_reports_deleted_ratchet_entries_as_no_longer_measured() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Stale(
                RatchetStaleViolation::Missing(RatchetStaleMissingViolation::File(
                    FileRatchetStaleMissingViolation {
                        kind: RatchetStaleViolationKind::FileLocStale,
                        path: RelativePath::new("src/deleted.rs").unwrap(),
                        ratchet: RatchetCount::new(4),
                    },
                )),
            ))]);

            let output = sarif_check_report(&report);
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
                    "message": "src/deleted.rs has stale ratchet ceiling 4 but is no longer measured.",
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

            let output = sarif_check_report(&report);
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
                        "message": "src in group source has stale ratchet ceiling 4 but is no longer measured.",
                    },
                    {
                        "group": "documentation",
                        "message": "src in group documentation has stale ratchet ceiling 5 but is no longer measured.",
                    },
                ])
            );
        }
    }
}
