import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { Result } from "better-result";
import { RuntimeFailureError } from "./errors.js";
import {
  MACHINE_OPERATIONAL_ERROR_EXIT_CODE,
  MACHINE_PROTOCOL_VERSION,
  MachineResponseResultKind,
  MachineRuntimeErrorCode,
} from "./machine-contract.js";
import { parseMachineResponse } from "./machine-response.js";

type MachineResultFixture = Record<string, unknown>;

function parseEnvelope(
  result: MachineResultFixture,
  protocolVersion: number = MACHINE_PROTOCOL_VERSION,
) {
  return parseMachineResponse(
    JSON.stringify({
      protocol_version: protocolVersion,
      result,
    }),
  );
}

function completedResult(overrides: MachineResultFixture = {}): MachineResultFixture {
  return {
    kind: MachineResponseResultKind.Completed,
    exit_code: 0,
    stdout: "",
    stderr: "",
    ...overrides,
  };
}

function requestErrorResult(overrides: MachineResultFixture = {}): MachineResultFixture {
  return {
    kind: MachineResponseResultKind.RequestError,
    exit_code: MACHINE_OPERATIONAL_ERROR_EXIT_CODE,
    stdout: "",
    stderr: "error: path must be relative\n",
    issues: [{ message: "path must be relative" }],
    ...overrides,
  };
}

function runtimeErrorResult(overrides: MachineResultFixture = {}): MachineResultFixture {
  return {
    kind: MachineResponseResultKind.RuntimeError,
    exit_code: MACHINE_OPERATIONAL_ERROR_EXIT_CODE,
    stdout: "",
    stderr: "config file does not exist: missing.toml\n",
    error_code: MachineRuntimeErrorCode.LibraryError,
    ...overrides,
  };
}

function assertParseFailure(response: ReturnType<typeof parseMachineResponse>): void {
  assert.ok(Result.isError(response));
  assert.ok(response.error instanceof RuntimeFailureError);
  assert.equal(response.error.operation, "parse machine response");
}

function assertParsedResult(
  response: ReturnType<typeof parseMachineResponse>,
  expected: MachineResultFixture,
): void {
  assert.ok(Result.isOk(response));
  assert.deepEqual(response.value, expected);
}

describe("parseMachineResponse", () => {
  describe("when the response is a completed machine envelope", () => {
    test("it returns the completed result", () => {
      const result = completedResult({
        exit_code: 1,
        stdout: "report\n",
      });
      const response = parseEnvelope(result);

      assertParsedResult(response, result);
    });

    test("it preserves the completed payload", () => {
      const result = completedResult({
        payload: {
          kind: "report",
          violations: [],
        },
      });
      const response = parseEnvelope(result);

      assertParsedResult(response, result);
    });
  });

  describe("when the response is a request_error machine envelope", () => {
    test("it returns the structured request error", () => {
      const result = requestErrorResult();
      const response = parseEnvelope(result);

      assertParsedResult(response, result);
    });
  });

  describe("when a request_error envelope claims a successful exit code", () => {
    test("it returns a structured runtime failure", () => {
      assertParseFailure(parseEnvelope(requestErrorResult({ exit_code: 0 })));
    });
  });

  describe("when a request_error envelope has no issues", () => {
    test("it returns a structured runtime failure", () => {
      assertParseFailure(parseEnvelope(requestErrorResult({ issues: [] })));
    });
  });

  describe("when a completed envelope uses an out-of-range exit code", () => {
    test("it returns a structured runtime failure", () => {
      assertParseFailure(parseEnvelope(completedResult({ exit_code: 256 })));
    });
  });

  describe("when the response is a runtime_error machine envelope", () => {
    test("it returns the structured runtime error", () => {
      const result = runtimeErrorResult();
      const response = parseEnvelope(result);

      assertParsedResult(response, result);
    });
  });

  describe("when a runtime_error envelope reports an impossible child exit code", () => {
    test("it returns the structured runtime error", () => {
      const result = runtimeErrorResult({
        stderr: "invalid machine exit code: 256\n",
        error_code: MachineRuntimeErrorCode.InvalidExitCode,
      });
      const response = parseEnvelope(result);

      assertParsedResult(response, result);
    });
  });

  describe("when a runtime_error envelope uses an unknown code", () => {
    test("it returns a structured runtime failure", () => {
      assertParseFailure(
        parseEnvelope(
          runtimeErrorResult({
            stderr: "runtime failure\n",
            error_code: "future_runtime_error",
          }),
        ),
      );
    });
  });

  describe("when a runtime_error envelope has an empty code", () => {
    test("it returns a structured runtime failure", () => {
      assertParseFailure(
        parseEnvelope(runtimeErrorResult({ stderr: "runtime failure\n", error_code: "" })),
      );
    });
  });

  describe("when a runtime_error envelope has a non-code discriminant", () => {
    test("it returns a structured runtime failure", () => {
      assertParseFailure(
        parseEnvelope(
          runtimeErrorResult({
            stderr: "runtime failure\n",
            error_code: "not a code\n",
          }),
        ),
      );
    });
  });

  describe("when a runtime_error envelope claims a successful exit code", () => {
    test("it returns a structured runtime failure", () => {
      assertParseFailure(parseEnvelope(runtimeErrorResult({ exit_code: 0 })));
    });
  });

  describe("when the protocol version is wrong", () => {
    test("it returns a structured runtime failure", () => {
      assertParseFailure(parseEnvelope(completedResult(), 2));
    });
  });
});
