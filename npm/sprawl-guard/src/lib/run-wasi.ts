import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { WASI } from "node:wasi";
import { Result } from "better-result";
import type { WasmModulePath } from "./paths.js";
import {
  RuntimeFailureError,
  type RuntimeOperation,
  type WrapperError,
} from "./errors.js";
import { buildMachineRunRequest } from "./machine-request.js";
import type { ProcessExit } from "./run-cli.js";
import {
  type CapturedWasiCommand,
  type CapturedWasiOutput,
  runWasiModuleWithHost,
} from "./run-wasi-core.js";

const FORWARDED_ENV_NAMES = [
  "CI",
  "CLICOLOR",
  "CLICOLOR_FORCE",
  "COLORTERM",
  "FORCE_COLOR",
  "GITHUB_ACTIONS",
  "NO_COLOR",
  "TERM",
] as const;

const MACHINE_REQUEST_OPERATION = "execute machine request" satisfies RuntimeOperation;

type ForwardedEnvironmentName = (typeof FORWARDED_ENV_NAMES)[number];

type MachineStdioFiles = {
  stderrFd: number;
  stderrPath: string;
  stdinFd: number;
  stdoutFd: number;
  stdoutPath: string;
  tempDirectory: string;
};

function filterEnvironment(): Partial<Record<ForwardedEnvironmentName, string>> {
  return Object.fromEntries(
    FORWARDED_ENV_NAMES.flatMap((name) => {
      const value = process.env[name];
      return value === undefined ? [] : [[name, value]];
    }),
  );
}

export async function runWasiModule(
  wasmPath: WasmModulePath,
  userArgv: string[],
): Promise<Result<ProcessExit, WrapperError>> {
  return runWasiModuleWithHost(
    {
      buildMachineRunRequest,
      runCapturedCommand,
      stderrWrite(chunk: string): void {
        process.stderr.write(chunk);
      },
      stdoutWrite(chunk: string): void {
        process.stdout.write(chunk);
      },
    },
    wasmPath,
    userArgv,
  );
}

async function runCapturedCommand(
  command: CapturedWasiCommand,
): Promise<Result<CapturedWasiOutput, WrapperError>> {
  const stdioFiles = prepareMachineStdioFiles(command.stdinContent);
  if (Result.isError(stdioFiles)) {
    return stdioFiles;
  }

  try {
    const wasi = new WASI({
      version: "preview1",
      args: command.args,
      env: filterEnvironment(),
      preopens: {
        "/workspace": command.hostRoot,
      },
      returnOnExit: true,
      stdin: stdioFiles.value.stdinFd,
      stdout: stdioFiles.value.stdoutFd,
      stderr: stdioFiles.value.stderrFd,
    });

    const exit = await startWasiModule(command.wasmPath, wasi);
    if (Result.isError(exit)) {
      return exit;
    }
    if (exit.value.kind !== "exit") {
      return Result.err(
        new RuntimeFailureError(
          MACHINE_REQUEST_OPERATION,
          `${MACHINE_REQUEST_OPERATION} did not terminate with a numeric exit code`,
          exit.value,
        ),
      );
    }

    const stdout = readMachineOutput(
      "read WASI stdout",
      stdioFiles.value.stdoutPath,
      "stdout",
    );
    if (Result.isError(stdout)) {
      return stdout;
    }

    const stderr = readMachineOutput(
      "read WASI stderr",
      stdioFiles.value.stderrPath,
      "stderr",
    );
    if (Result.isError(stderr)) {
      return stderr;
    }

    return Result.ok({
      exitCode: exit.value.code,
      stderr: stderr.value,
      stdout: stdout.value,
    });
  } finally {
    closeMachineStdioFiles(stdioFiles.value);
  }
}

function prepareMachineStdioFiles(
  stdinContent: string,
): Result<MachineStdioFiles, RuntimeFailureError> {
  let tempDirectory: string | undefined;
  const openedFds: number[] = [];

  try {
    tempDirectory = fs.mkdtempSync(
      path.join(os.tmpdir(), "sprawl-guard-wasi-"),
    );
    const stdinPath = path.join(tempDirectory, "stdin.json");
    const stdoutPath = path.join(tempDirectory, "stdout.txt");
    const stderrPath = path.join(tempDirectory, "stderr.txt");

    fs.writeFileSync(stdinPath, stdinContent, "utf8");

    const stderrFd = fs.openSync(stderrPath, "w");
    openedFds.push(stderrFd);
    const stdinFd = fs.openSync(stdinPath, "r");
    openedFds.push(stdinFd);
    const stdoutFd = fs.openSync(stdoutPath, "w");

    return Result.ok({
      stderrFd,
      stderrPath,
      stdinFd,
      stdoutFd,
      stdoutPath,
      tempDirectory,
    });
  } catch (cause) {
    cleanupPartialMachineStdioFiles(tempDirectory, openedFds);
    return Result.err(
      new RuntimeFailureError(
        "create WASI stdio files",
        "failed to prepare machine-mode stdio files for the WASI fallback",
        cause,
      ),
    );
  }
}

function readMachineOutput(
  operation: "read WASI stderr" | "read WASI stdout",
  filePath: string,
  streamName: "stderr" | "stdout",
): Result<string, RuntimeFailureError> {
  return Result.try({
    try: () => fs.readFileSync(filePath, "utf8"),
    catch: (cause) =>
      new RuntimeFailureError(
        operation,
        `failed to read machine-run ${streamName} from ${filePath}`,
        cause,
      ),
  });
}

function closeMachineStdioFiles(files: MachineStdioFiles): void {
  for (const fd of [files.stdinFd, files.stdoutFd, files.stderrFd]) {
    try {
      fs.closeSync(fd);
    } catch {
      // The primary command outcome should not be masked by cleanup failure.
    }
  }
  try {
    fs.rmSync(files.tempDirectory, {
      force: true,
      recursive: true,
    });
  } catch {
    // The primary command outcome should not be masked by cleanup failure.
  }
}

function cleanupPartialMachineStdioFiles(
  tempDirectory: string | undefined,
  fds: readonly number[],
): void {
  for (const fd of fds) {
    try {
      fs.closeSync(fd);
    } catch {
      // The setup failure is the actionable error; cleanup best-effort only.
    }
  }
  if (tempDirectory === undefined) {
    return;
  }
  try {
    fs.rmSync(tempDirectory, {
      force: true,
      recursive: true,
    });
  } catch {
    // The setup failure is the actionable error; cleanup best-effort only.
  }
}

async function startWasiModule(
  wasmPath: WasmModulePath,
  wasi: WASI,
): Promise<Result<ProcessExit, WrapperError>> {
  const wasmBytes = Result.try({
    try: () => fs.readFileSync(wasmPath),
    catch: (cause) =>
      new RuntimeFailureError(
        "read WASM module",
        `failed to read WASM module at ${wasmPath}`,
        cause,
      ),
  });
  if (Result.isError(wasmBytes)) {
    return wasmBytes;
  }

  const instantiatedModule = await Result.tryPromise({
    try: () =>
      WebAssembly.instantiate(wasmBytes.value, {
        wasi_snapshot_preview1: wasi.wasiImport,
      }),
    catch: (cause) =>
      new RuntimeFailureError(
        "instantiate WASM module",
        `failed to instantiate WASM module at ${wasmPath}`,
        cause,
      ),
  });
  if (Result.isError(instantiatedModule)) {
    return instantiatedModule;
  }

  return Result.try({
    try: () => wasi.start(instantiatedModule.value.instance),
    catch: (cause) =>
      new RuntimeFailureError(
        "start WASI module",
        `failed to execute WASI module at ${wasmPath}`,
        cause,
      ),
  }).map((code) => ({
    kind: "exit" as const,
    code,
  }));
}
