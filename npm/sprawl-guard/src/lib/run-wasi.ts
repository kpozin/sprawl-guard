import fs from "node:fs";
import { WASI } from "node:wasi";
import { Result } from "better-result";
import type { WasmModulePath } from "./paths.js";
import {
  RuntimeFailureError,
  type WrapperError,
} from "./errors.js";
import type { ProcessExit } from "./run-cli.js";
import { buildWasiInvocation, GUEST_ROOT } from "./wasi-invocation.js";

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

type ForwardedEnvironmentName = (typeof FORWARDED_ENV_NAMES)[number];

type WasiProcessExit = Extract<ProcessExit, { readonly kind: "exit" }>;

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
  const invocation = buildWasiInvocation(userArgv);
  if (Result.isError(invocation)) {
    return invocation;
  }

  const wasi = new WASI({
    version: "preview1",
    args: invocation.value.args,
    env: filterEnvironment(),
    preopens: {
      [GUEST_ROOT]: invocation.value.hostRoot,
    },
    returnOnExit: true,
  });
  return startWasiModule(wasmPath, wasi);
}

async function startWasiModule(
  wasmPath: WasmModulePath,
  wasi: WASI,
): Promise<Result<WasiProcessExit, WrapperError>> {
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
