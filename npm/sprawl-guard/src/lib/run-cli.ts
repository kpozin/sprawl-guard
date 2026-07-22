import fs from "node:fs";
import { spawnSync } from "node:child_process";
import { Result } from "better-result";
import {
  MissingFallbackPackageError,
  NativeBinaryMissingError,
  OverrideBinaryNotUsableError,
  type WrapperError,
  RuntimeFailureError,
} from "./errors.js";
import {
  resolveInstalledPackageRoot,
  resolveNativeBinaryPath,
  resolveNativePackageName,
  resolveWasmModulePath,
  WASM_PACKAGE_NAME,
  type NativePackageName,
} from "./resolve-package.js";
import {
  unsafeExecutablePathFromAbsolutePath,
  resolveAbsolutePath,
  type AbsolutePath,
  type ExecutablePath,
  type WasmModulePath,
} from "./paths.js";
import { startWithoutWasiExperimentalWarning } from "./wasi-warning.js";

export type ExecutionPlan =
  | {
      kind: "native";
      binaryPath: ExecutablePath;
    }
  | {
      kind: "wasi";
      userArgv: string[];
      wasmPath: WasmModulePath;
    };

export type ProcessExit =
  | {
      kind: "exit";
      code: number;
    }
  | {
      kind: "signal";
      signal: NodeJS.Signals;
    };

type WasiRunnerModule = typeof import("./run-wasi.js");

let wasiRunnerImport: Promise<WasiRunnerModule> | undefined;

/** Loads Node's WASI API without exposing its runtime warning as CLI stderr. */
function loadWasiRunner(): Promise<WasiRunnerModule> {
  wasiRunnerImport ??= startWithoutWasiExperimentalWarning(
    () => import("./run-wasi.js"),
  );
  return wasiRunnerImport;
}

function ensureUsableFile<TPath extends AbsolutePath | ExecutablePath>(
  filePath: TPath,
  errorFactory: () => WrapperError,
): Result<TPath, WrapperError> {
  const accessMode =
    process.platform === "win32" ? fs.constants.F_OK : fs.constants.X_OK;
  return Result.try({
    try: () => fs.accessSync(filePath, accessMode),
    catch: () => errorFactory(),
  }).map(() => filePath);
}

function resolveNativeExecutionPlan(
  packageName: NativePackageName,
): Result<ExecutionPlan | null, WrapperError> {
  const packageRoot = resolveInstalledPackageRoot(packageName);
  if (Result.isError(packageRoot)) {
    return packageRoot;
  }
  if (packageRoot.value === null) {
    return Result.ok(null);
  }

  const packageRootValue = packageRoot.value;
  const binaryPath = resolveNativeBinaryPath(packageRootValue);
  return ensureUsableFile(
    binaryPath,
    () => new NativeBinaryMissingError(packageName, binaryPath),
  ).map((binaryPath) => ({
    kind: "native" as const,
    binaryPath,
  }));
}

function resolveFallbackExecutionPlan(
  userArgv: string[],
  nativePackageName: NativePackageName | null,
): Result<ExecutionPlan, WrapperError> {
  const packageRoot = resolveInstalledPackageRoot(WASM_PACKAGE_NAME);
  if (Result.isError(packageRoot)) {
    return packageRoot;
  }
  if (packageRoot.value === null) {
    return Result.err(
      new MissingFallbackPackageError({
        packageName: WASM_PACKAGE_NAME,
        nativePackageName,
        platform: process.platform,
        arch: process.arch,
      }),
    );
  }

  return Result.ok({
    kind: "wasi",
    userArgv,
    wasmPath: resolveWasmModulePath(packageRoot.value),
  });
}

function resolveExecutionPlan(userArgv: string[]): Result<ExecutionPlan, WrapperError> {
  const overrideBinary = process.env.SPRAWL_GUARD_BINARY;
  if (overrideBinary) {
    const overrideBinaryPath = resolveAbsolutePath(overrideBinary);
    return ensureUsableFile(overrideBinaryPath, () => {
      return new OverrideBinaryNotUsableError(overrideBinaryPath);
    }).map((binaryPath) => ({
      kind: "native" as const,
      binaryPath: unsafeExecutablePathFromAbsolutePath(binaryPath),
    }));
  }

  const nativePackageName = resolveNativePackageName();
  if (nativePackageName !== null) {
    const nativePlan = resolveNativeExecutionPlan(nativePackageName);
    if (Result.isError(nativePlan)) {
      return nativePlan;
    }
    if (nativePlan.value !== null) {
      return Result.ok(nativePlan.value);
    }
  }

  return resolveFallbackExecutionPlan(userArgv, nativePackageName);
}

function runNativeBinary(
  binaryPath: ExecutablePath,
  userArgv: string[],
): Result<ProcessExit, RuntimeFailureError> {
  const spawned = Result.try({
    try: () =>
      spawnSync(binaryPath, userArgv, {
        stdio: "inherit",
      }),
    catch: (cause) =>
      new RuntimeFailureError(
        "spawn native binary",
        `failed to launch native binary ${binaryPath}`,
        cause,
      ),
  });

  if (Result.isError(spawned)) {
    return spawned;
  }

  if (spawned.value.error !== undefined) {
    return Result.err(
      new RuntimeFailureError(
        "execute native binary",
        `native binary ${binaryPath} returned a process execution error`,
        spawned.value.error,
      ),
    );
  }

  if (spawned.value.signal !== null) {
    return Result.ok({
      kind: "signal",
      signal: spawned.value.signal,
    });
  }

  return Result.ok({
    kind: "exit",
    code: spawned.value.status ?? 1,
  });
}

export async function runCli(userArgv: string[]): Promise<Result<ProcessExit, WrapperError>> {
  const executionPlan = resolveExecutionPlan(userArgv);
  if (Result.isError(executionPlan)) {
    return executionPlan;
  }

  if (executionPlan.value.kind === "native") {
    return runNativeBinary(executionPlan.value.binaryPath, userArgv);
  }

  const wasiRunner = await Result.tryPromise({
    try: loadWasiRunner,
    catch: (cause) =>
      new RuntimeFailureError(
        "load WASI fallback runner",
        "failed to load the WASI fallback runner",
        cause,
      ),
  });
  if (Result.isError(wasiRunner)) {
    return wasiRunner;
  }

  return wasiRunner.value.runWasiModule(
    executionPlan.value.wasmPath,
    executionPlan.value.userArgv,
  );
}
