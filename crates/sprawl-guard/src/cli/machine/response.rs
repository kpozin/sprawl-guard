use serde::{Serialize, Serializer};
use serde_json::Value;

use super::contract::{
    MACHINE_OPERATIONAL_ERROR_EXIT_CODE, MACHINE_PROTOCOL_VERSION, MachineRuntimeErrorCode,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct MachineResponse {
    protocol_version: u8,
    result: MachineResponseResult,
}

impl MachineResponse {
    pub(super) fn completed(completed: MachineCompleted) -> Self {
        Self {
            protocol_version: MACHINE_PROTOCOL_VERSION,
            result: MachineResponseResult::Completed(completed),
        }
    }

    pub(super) fn request_error(message: String) -> Self {
        Self {
            protocol_version: MACHINE_PROTOCOL_VERSION,
            result: MachineResponseResult::RequestError(MachineRequestError::new(message)),
        }
    }

    pub(super) fn runtime_error(error_code: MachineRuntimeErrorCode, stderr: String) -> Self {
        Self {
            protocol_version: MACHINE_PROTOCOL_VERSION,
            result: MachineResponseResult::RuntimeError(MachineRuntimeError::new(
                error_code, stderr,
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MachineResponseResult {
    Completed(MachineCompleted),
    RequestError(MachineRequestError),
    RuntimeError(MachineRuntimeError),
}

/// Returns the wire `kind` values emitted by the actual Serde response enum.
pub(super) fn machine_response_result_kind_wire_values() -> serde_json::Result<Vec<String>> {
    [
        MachineResponseResult::Completed(MachineCompleted::new(
            MachineExitCode::OPERATIONAL_ERROR,
            String::new(),
            String::new(),
            MachinePayload::Empty,
        )),
        MachineResponseResult::RequestError(MachineRequestError::new("request".to_owned())),
        MachineResponseResult::RuntimeError(MachineRuntimeError::new(
            MachineRuntimeErrorCode::NotImplemented,
            String::new(),
        )),
    ]
    .iter()
    .map(machine_response_result_kind_wire_value)
    .collect()
}

fn machine_response_result_kind_wire_value(
    result: &MachineResponseResult,
) -> serde_json::Result<String> {
    let value = serde_json::to_value(result)?;
    Ok(value
        .get("kind")
        .and_then(Value::as_str)
        .expect("machine response result serialization must emit a string kind")
        .to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct MachineOutput {
    exit_code: MachineExitCode,
    stdout: String,
    stderr: String,
}

impl MachineOutput {
    fn new(exit_code: MachineExitCode, stdout: String, stderr: String) -> Self {
        Self {
            exit_code,
            stdout,
            stderr,
        }
    }

    fn failed(stderr: String) -> Self {
        Self::new(MachineExitCode::OPERATIONAL_ERROR, String::new(), stderr)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct MachineExitCode(u8);

impl MachineExitCode {
    const OPERATIONAL_ERROR: Self = Self(MACHINE_OPERATIONAL_ERROR_EXIT_CODE);
}

impl TryFrom<i32> for MachineExitCode {
    type Error = i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        u8::try_from(value).map(Self).map_err(|_| value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct MachineCompleted {
    #[serde(flatten)]
    output: MachineOutput,
    #[serde(skip_serializing_if = "MachinePayload::is_empty")]
    pub(super) payload: MachinePayload,
}

impl MachineCompleted {
    pub(super) fn new(
        exit_code: MachineExitCode,
        stdout: String,
        stderr: String,
        payload: MachinePayload,
    ) -> Self {
        Self {
            output: MachineOutput::new(exit_code, stdout, stderr),
            payload,
        }
    }
}

/// Optional machine-readable command result with an explicit empty state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum MachinePayload {
    Empty,
    Json(Value),
}

impl MachinePayload {
    pub(super) fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

impl Serialize for MachinePayload {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Empty => serializer.serialize_unit(),
            Self::Json(value) => value.serialize(serializer),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct MachineRequestError {
    #[serde(flatten)]
    output: MachineOutput,
    issues: Vec<MachineRequestIssue>,
}

impl MachineRequestError {
    fn new(message: String) -> Self {
        let message = message.trim_end().to_owned();
        let stderr = if message.starts_with("error:") {
            format!("{message}\n")
        } else {
            format!("error: {message}\n")
        };
        Self {
            output: MachineOutput::failed(stderr),
            issues: vec![MachineRequestIssue { message }],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct MachineRequestIssue {
    message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct MachineRuntimeError {
    #[serde(flatten)]
    output: MachineOutput,
    error_code: MachineRuntimeErrorCode,
}

impl MachineRuntimeError {
    fn new(error_code: MachineRuntimeErrorCode, stderr: String) -> Self {
        Self {
            output: MachineOutput::failed(stderr),
            error_code,
        }
    }
}
