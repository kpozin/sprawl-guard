import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, test } from "node:test";
import { Result } from "better-result";
import { RuntimeFailureError } from "./errors.js";
import { runCli } from "./run-cli.js";
import { resolveNativePackageName, WASM_PACKAGE_NAME } from "./resolve-package.js";

const COMPILED_WRAPPER_ENTRYPOINT = fileURLToPath(
  new URL("../bin/sprawl-guard.js", import.meta.url),
);
const WRAPPER_PACKAGE_DIRECTORY = path.resolve(
  fileURLToPath(new URL(".", import.meta.url)),
  "..",
  "..",
);
const WASI_RUNNER_MODULE_PATH = path.join(
  WRAPPER_PACKAGE_DIRECTORY,
  "dist",
  "lib",
  "run-wasi.js",
);
const UNAVAILABLE_WASI_RUNNER_MODULE_PATH = `${WASI_RUNNER_MODULE_PATH}.unavailable`;
/** Minimal WASI module whose `_start` function calls `proc_exit(0)`. */
const WASI_EXIT_SUCCESS_MODULE = Buffer.from([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60,
  0x01, 0x7f, 0x00, 0x02, 0x24, 0x01, 0x16, 0x77, 0x61, 0x73, 0x69, 0x5f,
  0x73, 0x6e, 0x61, 0x70, 0x73, 0x68, 0x6f, 0x74, 0x5f, 0x70, 0x72, 0x65,
  0x76, 0x69, 0x65, 0x77, 0x31, 0x09, 0x70, 0x72, 0x6f, 0x63, 0x5f, 0x65,
  0x78, 0x69, 0x74, 0x00, 0x00, 0x03, 0x02, 0x01, 0x00, 0x07, 0x0a, 0x01,
  0x06, 0x5f, 0x73, 0x74, 0x61, 0x72, 0x74, 0x00, 0x00, 0x0a, 0x08, 0x01,
  0x06, 0x00, 0x41, 0x00, 0x10, 0x00, 0x0b,
]);

const createdRuntimePackageDirectories: string[] = [];
let wasiRunnerModuleHidden = false;

afterEach(() => {
  if (wasiRunnerModuleHidden) {
    fs.renameSync(UNAVAILABLE_WASI_RUNNER_MODULE_PATH, WASI_RUNNER_MODULE_PATH);
    wasiRunnerModuleHidden = false;
  }

  for (const packageDirectory of createdRuntimePackageDirectories.splice(0)) {
    fs.rmSync(packageDirectory, { force: true, recursive: true });
  }
});

function runtimePackageDirectory(packageName: string): string {
  return path.join(WRAPPER_PACKAGE_DIRECTORY, "node_modules", packageName);
}

function createRuntimePackage(packageName: string): string {
  const packageDirectory = runtimePackageDirectory(packageName);
  assert.equal(fs.existsSync(packageDirectory), false);
  fs.mkdirSync(packageDirectory, { recursive: true });
  fs.writeFileSync(
    path.join(packageDirectory, "package.json"),
    JSON.stringify({ name: packageName }),
  );
  createdRuntimePackageDirectories.push(packageDirectory);
  return packageDirectory;
}

function createNativeRuntimePackage(packageName: string): void {
  const packageDirectory = createRuntimePackage(packageName);
  const binaryName = process.platform === "win32" ? "sprawl-guard.exe" : "sprawl-guard";
  const binaryPath = path.join(packageDirectory, "bin", binaryName);
  fs.mkdirSync(path.dirname(binaryPath), { recursive: true });
  fs.copyFileSync(process.execPath, binaryPath);
  fs.chmodSync(binaryPath, 0o755);
}

function createWasiRuntimePackage(): void {
  const packageDirectory = createRuntimePackage(WASM_PACKAGE_NAME);
  fs.writeFileSync(path.join(packageDirectory, "sprawl-guard.wasm"), WASI_EXIT_SUCCESS_MODULE);
}

function wrapperEnvironment(overrides: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const environment = { ...process.env };
  delete environment.NODE_NO_WARNINGS;
  delete environment.NODE_OPTIONS;

  for (const [name, value] of Object.entries(overrides)) {
    if (value === undefined) {
      delete environment[name];
    } else {
      environment[name] = value;
    }
  }

  return environment;
}

function runCompiledWrapper(args: string[], overrides: NodeJS.ProcessEnv) {
  return spawnSync(process.execPath, [COMPILED_WRAPPER_ENTRYPOINT, ...args], {
    encoding: "utf8",
    env: wrapperEnvironment(overrides),
  });
}

describe("compiled wrapper entrypoint", () => {
  describe("when a native carrier is selected", () => {
    test("it does not load WASI or write its experimental warning", () => {
      const result = runCompiledWrapper(["--version"], {
        SPRAWL_GUARD_BINARY: process.execPath,
      });

      assert.equal(result.error, undefined);
      assert.equal(result.signal, null);
      assert.equal(result.status, 0);
      assert.match(result.stdout, /^v\d+\.\d+\.\d+/);
      assert.doesNotMatch(
        result.stderr,
        /ExperimentalWarning: WASI is an experimental feature/,
      );
    });

    test("it uses an installed optional package without the binary override", (context) => {
      const nativePackageName = resolveNativePackageName();
      if (nativePackageName === null) {
        context.skip("the current host has no supported native carrier");
        return;
      }
      createNativeRuntimePackage(nativePackageName);

      const result = runCompiledWrapper(["--version"], {
        SPRAWL_GUARD_BINARY: undefined,
      });

      assert.equal(result.error, undefined);
      assert.equal(result.signal, null);
      assert.equal(result.status, 0);
      assert.match(result.stdout, /^v\d+\.\d+\.\d+/);
      assert.doesNotMatch(
        result.stderr,
        /ExperimentalWarning: WASI is an experimental feature/,
      );
    });
  });

  describe("when only the WASI fallback is installed", () => {
    test("it suppresses the Node warning and reports its invocation failure", () => {
      createWasiRuntimePackage();

      const result = runCompiledWrapper([], {
        SPRAWL_GUARD_BINARY: undefined,
      });

      assert.equal(result.error, undefined);
      assert.equal(result.signal, null);
      assert.equal(result.status, 1);
      assert.doesNotMatch(
        result.stderr,
        /ExperimentalWarning: WASI is an experimental feature/,
      );
      assert.match(result.stderr, /failed to execute WASI module/);
    });

    test("it returns a typed runtime error when the fallback runner cannot load", async () => {
      createWasiRuntimePackage();
      fs.renameSync(WASI_RUNNER_MODULE_PATH, UNAVAILABLE_WASI_RUNNER_MODULE_PATH);
      wasiRunnerModuleHidden = true;

      const result = await runCli([]);

      assert.ok(Result.isError(result));
      assert.ok(result.error instanceof RuntimeFailureError);
      assert.equal(result.error.operation, "load WASI fallback runner");
    });
  });
});
