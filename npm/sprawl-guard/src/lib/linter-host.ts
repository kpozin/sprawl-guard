import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import type { AbsolutePath, GuestPath } from "./paths.js";
import type { WasiFileSystem } from "./wasi-invocation.js";

/** Import module name that the Sprawl Guard WASI build links against. */
export const LINTER_HOST_MODULE = "sprawl_guard_host";

/** One linter argument: literal text, or a file in a directory that exists only for the run. */
type LinterArgument = string | { readonly temporary: string };

/** A request to run a linter entry point found under the root or one of its parents. */
export type RunRequest = {
  readonly root: GuestPath;
  readonly entry: string;
  readonly args: readonly LinterArgument[];
};

/** Requests from the WASI build, matching `host::Request` in the Rust CLI. */
export type LinterHostRequest =
  | { readonly run: RunRequest; readonly guestPath?: never }
  | { readonly guestPath: { readonly path: AbsolutePath }; readonly run?: never };

/** Outcome of a run request, matching `host::RunResponse` in the Rust CLI. */
export type RunResponse =
  | { readonly exited: { readonly code: number | null; readonly stdout: string; readonly stderr: string } }
  | "missingEntry"
  | { readonly temporaryFailed: { readonly message: string } }
  | { readonly launchFailed: { readonly message: string } };

/**
 * Host functions that run project linters for the WASI build, which cannot start processes.
 *
 * The module sends a JSON request and receives the length of the JSON response, then calls
 * `read_response` with a buffer of that length.
 */
export function linterHostImports(
  fileSystem: WasiFileSystem,
  memory: () => WebAssembly.Memory,
): WebAssembly.ModuleImports {
  let pending = new Uint8Array();
  return {
    send_request(pointer: number, length: number): number {
      // WebAssembly passes `i32` values to JavaScript as signed numbers.
      const text = Buffer.from(memory().buffer, pointer >>> 0, length >>> 0).toString("utf8");
      // The module ships in the same release as this wrapper, so its requests are as trusted as
      // its code; a mismatch is a packaging bug that traps the module.
      const request = JSON.parse(text) as LinterHostRequest;
      const response =
        request.run === undefined
          ? (fileSystem.toGuest(request.guestPath.path) ?? request.guestPath.path)
          : runLinter(fileSystem, request.run);
      pending = Buffer.from(JSON.stringify(response), "utf8");
      return pending.length;
    },
    read_response(pointer: number): void {
      new Uint8Array(memory().buffer, pointer >>> 0, pending.length).set(pending);
      pending = new Uint8Array();
    },
  };
}

function runLinter(fileSystem: WasiFileSystem, { root, entry, args }: RunRequest): RunResponse {
  const hostRoot = fileSystem.toHost(root);
  if (hostRoot === undefined) {
    return { launchFailed: { message: `the wrapper cannot find the host path of ${root}` } };
  }
  const entryPath = ancestors(hostRoot)
    .map((directory) => path.join(directory, ...entry.split("/")))
    .find(isFile);
  if (entryPath === undefined) {
    return "missingEntry";
  }
  // Created on first use, so runs without temporary files never touch the temporary directory.
  let temporary: string | undefined;
  try {
    let argv: string[];
    try {
      argv = args.map((arg) =>
        typeof arg === "string"
          ? arg
          : path.join((temporary ??= fs.mkdtempSync(path.join(os.tmpdir(), "sprawl-guard-"))), arg.temporary),
      );
    } catch (error) {
      return { temporaryFailed: { message: error instanceof Error ? error.message : String(error) } };
    }
    // Match the native build, which buffers linter output without a size limit.
    const result = spawnSync(process.execPath, [entryPath, ...argv], {
      cwd: hostRoot,
      encoding: "utf8",
      maxBuffer: Number.POSITIVE_INFINITY,
    });
    return result.error === undefined
      ? {
          exited: {
            // Windows reports some exit codes above `i32::MAX`; the native build reads them as
            // signed 32-bit values.
            code: result.status === null ? null : result.status | 0,
            stdout: result.stdout,
            stderr: result.stderr,
          },
        }
      : { launchFailed: { message: result.error.message } };
  } finally {
    if (temporary !== undefined) {
      fs.rmSync(temporary, { recursive: true, force: true });
    }
  }
}

/** Whether `candidate` is a file, treating any failure to inspect it as "no", like Rust's `is_file`. */
function isFile(candidate: string): boolean {
  try {
    return fs.statSync(candidate).isFile();
  } catch {
    return false;
  }
}

function ancestors(directory: string): string[] {
  const parent = path.dirname(directory);
  return parent === directory ? [directory] : [directory, ...ancestors(parent)];
}
