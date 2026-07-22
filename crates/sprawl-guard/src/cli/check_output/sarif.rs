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
            "A production source file exceeded its resolved code-line limit.",
        ),
        rule(
            TEST_FILE_LOC_RULE_ID,
            "Test file LOC budget exceeded",
            "A path-classified test file exceeded its resolved code-line limit.",
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
                "{} has {} code lines, exceeding limit {}.",
                violation.path, violation.actual, violation.limit
            )
        },
        "locations": [location(&violation.path)],
        "properties": {
            "kind": violation.kind,
            "path": violation.path,
            "language": violation.language,
            "actual": violation.actual,
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
                "{} has {} direct {source_kind} leaf files, exceeding limit {}.",
                display_path(&violation.path), violation.actual, violation.limit
            )
        },
        "locations": [location(&violation.path)],
        "properties": {
            "kind": violation.kind,
            "path": violation.path,
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
    json!({
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
    })
}

fn ratchet_message(violation: &RatchetViolation) -> String {
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
                "{} increased to {actual} above ratchet ceiling {}.",
                display_path(violation.path()),
                violation.ratchet().get()
            )
        }
        RatchetViolationKind::FileLocStale
        | RatchetViolationKind::TestFileLocStale
        | RatchetViolationKind::DirectoryFanoutStale
        | RatchetViolationKind::TestDirectoryFanoutStale => match violation.actual() {
            Some(actual) => format!(
                "{} has stale ratchet ceiling {}; current value is {}.",
                display_path(violation.path()),
                violation.ratchet().get(),
                actual.get()
            ),
            None => format!(
                "{} has stale ratchet ceiling {} but is no longer measured.",
                display_path(violation.path()),
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
        FileViolationKind, RatchetIncreasedViolation, RatchetIncreasedViolationKind,
        RatchetStaleMeasuredViolation, RatchetStaleMissingViolation, RatchetStaleViolation,
        RatchetStaleViolationKind, RatchetViolation,
    };
    use sprawl_guard_lib::{
        CheckWarning, CheckWarningKind, CodeLines, CountedCodeLines, DiagnosticDetail, LanguageId,
        LeafFiles, RatchetCount, RelativePath,
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
            actual: CountedCodeLines::new(3),
            limit: CodeLines::new(2).unwrap(),
            ratchet: None,
        }
    }

    fn directory_fanout_violation(
        kind: DirectoryViolationKind,
        path: &str,
        files: Vec<&str>,
    ) -> DirectoryFanoutViolation {
        DirectoryFanoutViolation {
            kind,
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
                                    "text": "A production source file exceeded its resolved code-line limit."
                                }
                            },
                            {
                                "id": "sprawl-guard/test-file-loc",
                                "shortDescription": {
                                    "text": "Test file LOC budget exceeded"
                                },
                                "fullDescription": {
                                    "text": "A path-classified test file exceeded its resolved code-line limit."
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
                    "location": result["locations"][0]["physicalLocation"],
                    "properties": {
                        "language": result["properties"]["language"],
                        "actual": result["properties"]["actual"],
                    },
                }),
                json!({
                    "ruleId": FILE_LOC_RULE_ID,
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
                RatchetIncreasedViolation {
                    kind: RatchetIncreasedViolationKind::FileLocIncreased,
                    path: RelativePath::new("src/lib.rs").unwrap(),
                    actual: RatchetCount::new(5),
                    limit: RatchetCount::new(2),
                    ratchet: RatchetCount::new(4),
                },
            ))]);

            let output = sarif_check_report(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(
                json!({
                    "ruleId": result["ruleId"],
                    "properties": {
                        "actual": result["properties"]["actual"],
                        "limit": result["properties"]["limit"],
                        "ratchet": result["properties"]["ratchet"],
                    },
                }),
                json!({
                    "ruleId": RATCHET_INCREASE_RULE_ID,
                    "properties": {
                        "actual": 5,
                        "limit": 2,
                        "ratchet": 4,
                    },
                })
            );
        }

        #[test]
        fn it_maps_stale_entries_to_the_ratchet_stale_rule() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Stale(
                RatchetStaleViolation::Measured(RatchetStaleMeasuredViolation {
                    kind: RatchetStaleViolationKind::DirectoryFanoutStale,
                    path: RelativePath::new("src").unwrap(),
                    actual: RatchetCount::new(1),
                    limit: Some(RatchetCount::new(2)),
                    ratchet: RatchetCount::new(4),
                }),
            ))]);

            let output = sarif_check_report(&report);
            let result = &output["runs"][0]["results"][0];

            assert_eq!(result["ruleId"], RATCHET_STALE_RULE_ID);
            assert_eq!(result["properties"]["kind"], "directory_fanout_stale");
        }

        #[test]
        fn it_reports_deleted_ratchet_entries_as_no_longer_measured() {
            let report = report_with(vec![CheckViolation::Ratchet(RatchetViolation::Stale(
                RatchetStaleViolation::Missing(RatchetStaleMissingViolation {
                    kind: RatchetStaleViolationKind::FileLocStale,
                    path: RelativePath::new("src/deleted.rs").unwrap(),
                    ratchet: RatchetCount::new(4),
                }),
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
    }
}
