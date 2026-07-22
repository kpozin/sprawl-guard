import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { Result } from "better-result";
import {
  MissingFlagValueError,
  RuntimeFailureError,
  type WrapperError,
} from "./errors.js";
import {
  MACHINE_OPERATIONAL_ERROR_EXIT_CODE,
  MACHINE_PROTOCOL_VERSION,
  MachineHostPathStyle,
  MachineResponseResultKind,
} from "./machine-contract.js";
import type { MachineRunPlan } from "./machine-request.js";
import {
  packageRootPathFromPackageJsonPath,
  resolveAbsolutePath,
  resolveWasmModulePath,
  unsafeAbsolutePathFromKnownAbsolute,
  unsafeHostRootPathFromAbsolutePath,
} from "./paths.js";
import {
  type CapturedWasiCommand,
  type CapturedWasiOutput,
  runWasiModuleWithHost,
} from "./run-wasi-core.js";

type HostHarness = {
  capturedCommands: CapturedWasiCommand[];
  host: {
    buildMachineRunRequest(argv: readonly string[]): Result<MachineRunPlan, WrapperError>;
    runCapturedCommand(
      command: CapturedWasiCommand,
    ): Promise<Result<CapturedWasiOutput, WrapperError>>;
    stderrWrite(chunk: string): void;
    stdoutWrite(chunk: string): void;
  };
  stderrChunks: string[];
  stdoutChunks: string[];
};

type MachineResultFixture = Record<string, unknown>;

function machineStdout(result: MachineResultFixture): string {
  return JSON.stringify({
    protocol_version: MACHINE_PROTOCOL_VERSION,
    result,
  });
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

function buildTestMachinePlan(): MachineRunPlan {
  return {
    hostPathStyle: MachineHostPathStyle.Posix,
    hostRoot: unsafeHostRootPathFromAbsolutePath(resolveAbsolutePath("/repo")),
    requestJson: JSON.stringify({
      protocol_version: MACHINE_PROTOCOL_VERSION,
      workspace_root: "/workspace",
      host_path_mapping: {
        kind: "alias",
        host_root: "/repo",
        style: MachineHostPathStyle.Posix,
      },
      argv: ["check", "--format", "json"],
    }),
  };
}

function buildTestWasmPath() {
  return resolveWasmModulePath(
    packageRootPathFromPackageJsonPath(
      unsafeAbsolutePathFromKnownAbsolute("/pkg/package.json"),
    ),
  );
}

function createHarness(args: {
  capturedResults?: Array<Result<CapturedWasiOutput, WrapperError>>;
  machinePlan: Result<MachineRunPlan, WrapperError>;
}): HostHarness {
  const capturedCommands: CapturedWasiCommand[] = [];
  const capturedResults = [...(args.capturedResults ?? [])];
  const stderrChunks: string[] = [];
  const stdoutChunks: string[] = [];

  return {
    capturedCommands,
    host: {
      buildMachineRunRequest(): Result<MachineRunPlan, WrapperError> {
        return args.machinePlan;
      },
      async runCapturedCommand(
        command: CapturedWasiCommand,
      ): Promise<Result<CapturedWasiOutput, WrapperError>> {
        capturedCommands.push(command);
        const result = capturedResults.shift();
        if (result === undefined) {
          assert.fail("unexpected captured command");
        }
        return result;
      },
      stderrWrite(chunk: string): void {
        stderrChunks.push(chunk);
      },
      stdoutWrite(chunk: string): void {
        stdoutChunks.push(chunk);
      },
    },
    stderrChunks,
    stdoutChunks,
  };
}

describe("runWasiModuleWithHost", () => {
  const wasmPath = buildTestWasmPath();
  const machinePlan = buildTestMachinePlan();

  describe("when the wrapper cannot build a machine request", () => {
    test("it returns the wrapper error without invoking WASI", async () => {
      const harness = createHarness({
        machinePlan: Result.err(new MissingFlagValueError("--root")),
      });

      const result = await runWasiModuleWithHost(harness.host, wasmPath, ["check"]);

      assert.ok(Result.isError(result));
      assert.ok(result.error instanceof MissingFlagValueError);
      assert.deepEqual(harness.capturedCommands, []);
    });
  });

  describe("when machine run returns a completed envelope", () => {
    test("it relays the machine response streams", async () => {
      const harness = createHarness({
        capturedResults: [
          Result.ok({
            exitCode: 0,
            stderr: "",
            stdout: machineStdout(
              completedResult({
                exit_code: 1,
                stdout: "report\n",
                stderr: "warning\n",
              }),
            ),
          }),
        ],
        machinePlan: Result.ok(machinePlan),
      });

      const result = await runWasiModuleWithHost(harness.host, wasmPath, ["check"]);

      assert.deepEqual(result, Result.ok({ kind: "exit", code: 1 }));
      assert.deepEqual(harness.stdoutChunks, ["report\n"]);
      assert.deepEqual(harness.stderrChunks, ["warning\n"]);
      assert.deepEqual(
        harness.capturedCommands.map((command) => command.args),
        [["sprawl-guard", "machine", "run"]],
      );
    });
  });

  describe("when machine run returns a request_error envelope", () => {
    test("it relays the envelope streams and exit code", async () => {
      const harness = createHarness({
        capturedResults: [
          Result.ok({
            exitCode: 0,
            stderr: "",
            stdout: machineStdout(requestErrorResult()),
          }),
        ],
        machinePlan: Result.ok(machinePlan),
      });

      const result = await runWasiModuleWithHost(harness.host, wasmPath, ["check"]);

      assert.deepEqual(
        result,
        Result.ok({ kind: "exit", code: MACHINE_OPERATIONAL_ERROR_EXIT_CODE }),
      );
      assert.deepEqual(harness.stdoutChunks, []);
      assert.deepEqual(harness.stderrChunks, ["error: path must be relative\n"]);
    });
  });

  describe("when machine run exits nonzero", () => {
    test("it returns a runtime failure instead of parsing stdout as a response", async () => {
      const harness = createHarness({
        capturedResults: [
          Result.ok({
            exitCode: 2,
            stderr: "runtime transport failure\n",
            stdout: machineStdout(completedResult({ stdout: "report\n" })),
          }),
        ],
        machinePlan: Result.ok(machinePlan),
      });

      const result = await runWasiModuleWithHost(harness.host, wasmPath, ["check"]);

      assert.ok(Result.isError(result));
      assert.ok(result.error instanceof RuntimeFailureError);
      assert.equal(result.error.operation, "execute machine request");
      assert.deepEqual(harness.stdoutChunks, []);
      assert.deepEqual(harness.stderrChunks, []);
    });
  });

  describe("when machine run writes raw stderr", () => {
    test("it returns a runtime failure instead of treating the response as valid", async () => {
      const harness = createHarness({
        capturedResults: [
          Result.ok({
            exitCode: 0,
            stderr: "unexpected stderr\n",
            stdout: machineStdout(completedResult()),
          }),
        ],
        machinePlan: Result.ok(machinePlan),
      });

      const result = await runWasiModuleWithHost(harness.host, wasmPath, ["check"]);

      assert.ok(Result.isError(result));
      assert.ok(result.error instanceof RuntimeFailureError);
      assert.equal(result.error.operation, "execute machine request");
      assert.deepEqual(harness.stdoutChunks, []);
      assert.deepEqual(harness.stderrChunks, []);
    });
  });
});
