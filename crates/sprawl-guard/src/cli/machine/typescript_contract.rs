use askama::Template;
use serde::Serialize;

use super::contract::{
    MACHINE_MAX_PROCESS_EXIT_CODE, MACHINE_OPERATIONAL_ERROR_EXIT_CODE, MACHINE_PROTOCOL_VERSION,
    MachineHostPathStyle, MachineRuntimeErrorCode,
};
use super::response::machine_response_result_kind_wire_values;
use crate::error::{CliError, Result};

// Askama reads this file during macro expansion. Keep an explicit source-level include so
// template-only edits also invalidate the Rust compile step.
const _: &[u8] = include_bytes!("../../../templates/machine-contract.ts.askama");

/// Renders the TypeScript module consumed by the NPM wrapper.
pub(super) fn render_machine_contract_typescript() -> Result<String> {
    let template = MachineContractTemplate {
        protocol_version: MACHINE_PROTOCOL_VERSION,
        max_process_exit_code: MACHINE_MAX_PROCESS_EXIT_CODE,
        operational_error_exit_code: MACHINE_OPERATIONAL_ERROR_EXIT_CODE,
        host_path_styles: enum_members_from_serializable_values(
            "MachineHostPathStyle",
            MachineHostPathStyle::VARIANTS,
        )?,
        response_result_kinds: enum_members_from_wire_values(
            machine_response_result_kind_wire_values()
                .map_err(|source| CliError::RenderJson { source })?,
        ),
        runtime_error_codes: enum_members_from_serializable_values(
            "MachineRuntimeErrorCode",
            MachineRuntimeErrorCode::VARIANTS,
        )?,
    };
    let mut rendered = template
        .render()
        .map_err(|source| CliError::RenderMachineContract { source })?;
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

#[derive(Template)]
#[template(path = "machine-contract.ts.askama", escape = "none")]
struct MachineContractTemplate {
    protocol_version: u8,
    max_process_exit_code: u8,
    operational_error_exit_code: u8,
    host_path_styles: Vec<TsEnumMember>,
    response_result_kinds: Vec<TsEnumMember>,
    runtime_error_codes: Vec<TsEnumMember>,
}

struct TsEnumMember {
    name: String,
    value: String,
}

fn enum_members_from_serializable_values<T>(
    name: &'static str,
    values: impl IntoIterator<Item = T>,
) -> Result<Vec<TsEnumMember>>
where
    T: Serialize,
{
    values
        .into_iter()
        .map(|value| {
            let wire_value = serializable_wire_string(name, value)?;
            Ok(enum_member_from_wire_value(&wire_value))
        })
        .collect()
}

fn enum_members_from_wire_values(values: impl IntoIterator<Item = String>) -> Vec<TsEnumMember> {
    values
        .into_iter()
        .map(|value| enum_member_from_wire_value(&value))
        .collect()
}

fn enum_member_from_wire_value(wire_value: &str) -> TsEnumMember {
    TsEnumMember {
        name: pascal_case_wire_value(wire_value),
        value: json_string_literal(wire_value).expect("string literal serialization cannot fail"),
    }
}

fn serializable_wire_string<T>(name: &'static str, value: T) -> Result<String>
where
    T: Serialize,
{
    let value = serde_json::to_value(value).map_err(|source| CliError::RenderJson { source })?;
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or(CliError::MachineContractValueNonString { name })
}

fn json_string_literal(value: &str) -> serde_json::Result<String> {
    serde_json::to_string(value)
}

fn pascal_case_wire_value(value: &str) -> String {
    let mut output = String::new();
    let mut capitalize_next = true;
    for character in value.chars() {
        if matches!(character, '_' | '-' | ' ') {
            capitalize_next = true;
            continue;
        }
        if capitalize_next {
            output.extend(character.to_uppercase());
            capitalize_next = false;
        } else {
            output.push(character);
        }
    }
    output
}
