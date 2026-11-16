import fs from "node:fs";
import { WASI } from "node:wasi";
import { Result } from "better-result";
import type { WasmModulePath } from "./paths.js";
import {
  RuntimeFailureError,
  type WrapperError,
} from "./errors.js";
import { linterHostImports, LINTER_HOST_MODULE } from "./linter-host.js";
import type { ProcessExit } from "./run-cli.js";
import { withDirectoryListing, withNativeLinkFollowing } from "./windows-wasi.js";
import { buildWasiInvocation, type WasiFileSystem } from "./wasi-invocation.js";

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
  // Node opens every preopen here, so a volume that does not exist fails before the module runs.
  const wasi = Result.try({
    try: () =>
      new WASI({
        version: "preview1",
        args: invocation.args,
        env: { ...filterEnvironment(), ...invocation.env },
        preopens: Object.fromEntries(invocation.fileSystem.preopens),
        returnOnExit: true,
      }),
    catch: (cause) =>
      new RuntimeFailureError(
        "open WASI directories",
        `failed to open ${[...invocation.fileSystem.preopens.values()].join(", ")} for the WASM module`,
        cause,
      ),
  });
  if (Result.isError(wasi)) {
    return wasi;
  }
  return startWasiModule(wasmPath, wasi.value, invocation.fileSystem);
}

async function startWasiModule(
  wasmPath: WasmModulePath,
  wasi: WASI,
  fileSystem: WasiFileSystem,
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

  // Host functions run only after instantiation, once the module's memory exists.
  let exports: WebAssembly.Exports = {};
  const memory = () => exports.memory as WebAssembly.Memory;
  const instantiatedModule = await Result.tryPromise({
    try: () =>
      WebAssembly.instantiate(wasmBytes.value, {
        // On Windows, Node's WASI cannot list directories (see `UVWASI_FD_READDIR_SUPPORTED` in
        // uvwasi) or follow links whose targets start with a drive letter. Once it can, the module
        // can use `wasi.wasiImport` directly.
        wasi_snapshot_preview1:
          process.platform === "win32"
            ? withNativeLinkFollowing(withDirectoryListing(wasi.wasiImport, fileSystem, memory), fileSystem, memory)
            : wasi.wasiImport,
        [LINTER_HOST_MODULE]: linterHostImports(fileSystem, memory),
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
  exports = instantiatedModule.value.instance.exports;

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
