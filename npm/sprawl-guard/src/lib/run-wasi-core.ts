import { Result } from "better-result";
import {
  RuntimeFailureError,
  type WrapperError,
} from "./errors.js";
import { type MachineRunPlan, buildMachineRunRequest } from "./machine-request.js";
import { parseMachineResponse } from "./machine-response.js";
import type { HostRootPath, WasmModulePath } from "./paths.js";
import type { ProcessExit } from "./run-cli.js";

/** Structured output captured from one machine-oriented WASI command. */
export type CapturedWasiOutput = {
  exitCode: number;
  stderr: string;
  stdout: string;
};

/** Internal command description for machine-mode WASI invocations. */
export type CapturedWasiCommand = {
  args: string[];
  hostRoot: HostRootPath;
  stdinContent: string;
  wasmPath: WasmModulePath;
};

/** Output sink used while relaying machine response streams back to the host process. */
export type WasiOutputSink = {
  stderrWrite(chunk: string): void;
  stdoutWrite(chunk: string): void;
};

/** Internal orchestration dependencies for the WASI fallback wrapper path. */
export type WasiModuleHost = WasiOutputSink & {
  buildMachineRunRequest: typeof buildMachineRunRequest;
  runCapturedCommand(
    command: CapturedWasiCommand,
  ): Promise<Result<CapturedWasiOutput, WrapperError>>;
};

/** Shared orchestration for `runWasiModule`, separated so tests can cover real control flow. */
export async function runWasiModuleWithHost(
  host: WasiModuleHost,
  wasmPath: WasmModulePath,
  userArgv: string[],
): Promise<Result<ProcessExit, WrapperError>> {
  const machinePlan = host.buildMachineRunRequest(userArgv);
  if (Result.isError(machinePlan)) {
    return machinePlan;
  }

  return runMachineRequest(host, wasmPath, machinePlan.value);
}

async function runMachineRequest(
  host: WasiModuleHost,
  wasmPath: WasmModulePath,
  machinePlan: MachineRunPlan,
): Promise<Result<ProcessExit, WrapperError>> {
  const captured = await host.runCapturedCommand({
    args: ["sprawl-guard", "machine", "run"],
    hostRoot: machinePlan.hostRoot,
    stdinContent: machinePlan.requestJson,
    wasmPath,
  });
  if (Result.isError(captured)) {
    return captured;
  }
  if (captured.value.exitCode !== 0) {
    return Result.err(
      new RuntimeFailureError(
        "execute machine request",
        "machine run exited nonzero instead of returning a response envelope",
        {
          exitCode: captured.value.exitCode,
          stderr: captured.value.stderr,
          stdout: captured.value.stdout,
        },
      ),
    );
  }
  if (captured.value.stderr.length > 0) {
    return Result.err(
      new RuntimeFailureError(
        "execute machine request",
        "machine run wrote raw stderr instead of using the response envelope",
        captured.value.stderr,
      ),
    );
  }

  const response = parseMachineResponse(captured.value.stdout);
  if (Result.isError(response)) {
    return response;
  }

  if (response.value.stdout.length > 0) {
    host.stdoutWrite(response.value.stdout);
  }
  if (response.value.stderr.length > 0) {
    host.stderrWrite(response.value.stderr);
  }
  return Result.ok({
    kind: "exit",
    code: response.value.exit_code,
  });
}
