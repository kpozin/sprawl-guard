use super::common::*;

mod when_a_schema_is_requested {
    use super::*;

    #[test]
    fn config_prints_the_authored_configuration_contract() {
        let output = sprawl_guard_no_root()
            .args(["schema", "config"])
            .output()
            .unwrap();
        let schema = json_stdout(&output);

        assert_exit_code(&output, exit_codes::SUCCESS);
        let contract = serde_json::json!({
            "$schema": schema["$schema"],
            "id": schema["id"],
            "has_later_draft_id": schema.get("$id").is_some(),
            "title": schema["title"],
            "type": schema["type"],
            "additional_properties": schema["additionalProperties"],
            "required": schema["required"],
            "schema_version": schema["properties"]["schema_version"]["enum"],
            "rust_language_schema": schema["properties"]["languages"]["properties"]["rust"],
            "python_language_schema": schema["properties"]["languages"]["properties"]["python"],
            "other_language_schema": schema["properties"]["languages"]["additionalProperties"],
            "override_required": schema["definitions"]["PathOverride"]["required"],
            "has_output_color": schema["definitions"]["PartialOutputConfig"]["properties"]
                .get("color")
                .is_some(),
        });
        assert_eq!(
            contract,
            serde_json::json!({
                "$schema": "http://json-schema.org/draft-04/schema#",
                "id": "https://kpozin.github.io/sprawl-guard/schemas/v2/sprawl-guard.schema.json",
                "has_later_draft_id": false,
                "title": "Sprawl Guard configuration",
                "type": "object",
                "additional_properties": false,
                "required": ["schema_version"],
                "schema_version": [2],
                "rust_language_schema": {"$ref": "#/definitions/PartialLanguageConfig"},
                "python_language_schema": {"$ref": "#/definitions/PartialCommonLanguageConfig"},
                "other_language_schema": {"$ref": "#/definitions/PartialCommonLanguageConfig"},
                "override_required": ["paths", "reason"],
                "has_output_color": false,
            })
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn ratchet_prints_a_distinct_authored_ratchet_contract() {
        let output = sprawl_guard_no_root()
            .args(["schema", "ratchet"])
            .output()
            .unwrap();
        let schema = json_stdout(&output);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            serde_json::json!({
                "$schema": schema["$schema"],
                "id": schema["id"],
                "title": schema["title"],
                "type": schema["type"],
                "additional_properties": schema["additionalProperties"],
                "required": schema["required"],
                "schema_version": schema["properties"]["schema_version"]["enum"],
                "files_ref": schema["properties"]["files"]["items"]["$ref"],
                "file_path_min_length": schema["definitions"]["FileRatchet"]["properties"]["path"]["allOf"][1]["minLength"],
                "directories_ref": schema["properties"]["directories"]["items"]["$ref"],
                "has_config": schema["properties"].get("config").is_some(),
            }),
            serde_json::json!({
                "$schema": "http://json-schema.org/draft-04/schema#",
                "id": "https://kpozin.github.io/sprawl-guard/schemas/v1/sprawl-guard.ratchet.schema.json",
                "title": "Sprawl Guard ratchet",
                "type": "object",
                "additional_properties": false,
                "required": ["schema_version"],
                "schema_version": [1],
                "files_ref": "#/definitions/FileRatchet",
                "file_path_min_length": 1,
                "directories_ref": "#/definitions/DirectoryRatchet",
                "has_config": false,
            })
        );
        assert_eq!(stderr(&output), "");
    }
}
