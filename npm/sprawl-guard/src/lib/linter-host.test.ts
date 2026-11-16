import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, describe, test } from "node:test";
import { linterHostImports, type RunRequest, type RunResponse } from "./linter-host.js";
import { unsafeAbsolutePathFromKnownAbsolute as absolute, type GuestPath } from "./paths.js";
import { type WasiFileSystem, wasiFileSystem } from "./wasi-invocation.js";

/** Byte offset where test responses are written, past any request. */
const RESPONSE_OFFSET = 32_768;

/** A linter stand-in that reports how it was started. */
const REPORTING_LINTER = `
const fs = require("node:fs");
const path = require("node:path");
const args = process.argv.slice(2);
console.log(JSON.stringify({
  cwd: process.cwd(),
  args,
  temporaryDirectoryExists: args.length > 1 && fs.existsSync(path.dirname(args[1])),
}));
process.exit(3);
`;

/** What the reporting linter prints. */
type LinterReport = {
  readonly cwd: string;
  readonly args: readonly string[];
  readonly temporaryDirectoryExists: boolean;
};

const memory = new WebAssembly.Memory({ initial: 1 });

/** Sends one request through the host imports, as the WASI module does. */
function send(fileSystem: WasiFileSystem, request: { readonly run: RunRequest }): RunResponse;
function send(fileSystem: WasiFileSystem, request: { readonly guestPath: { readonly path: string } }): string;
function send(fileSystem: WasiFileSystem, request: unknown): unknown {
  const imports = linterHostImports(fileSystem, () => memory);
  const bytes = Buffer.from(JSON.stringify(request), "utf8");
  new Uint8Array(memory.buffer).set(bytes);
  const length = (imports.send_request as (pointer: number, length: number) => number)(0, bytes.length);
  (imports.read_response as (pointer: number) => void)(RESPONSE_OFFSET);
  return JSON.parse(Buffer.from(memory.buffer, RESPONSE_OFFSET, length).toString("utf8"));
}

describe("linterHostImports", () => {
  const workspace = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "linter-host-test-")));
  after(() => fs.rmSync(workspace, { recursive: true, force: true }));
  const project = path.join(workspace, "project");
  const root = path.join(project, "packages", "app");
  fs.mkdirSync(root, { recursive: true });
  fs.mkdirSync(path.join(project, "node_modules", "fake", "bin"), { recursive: true });
  fs.writeFileSync(path.join(project, "node_modules", "fake", "bin", "fake.js"), REPORTING_LINTER);
  // A stray file named `node_modules` between the root and the installed package.
  fs.writeFileSync(path.join(project, "packages", "node_modules"), "");

  const fileSystem = wasiFileSystem(process.platform, absolute(workspace));
  const guest = (hostPath: string) => fileSystem.toGuest(absolute(hostPath)) as GuestPath;

  /** Asks the host to run the fake linter, or another entry point, from `hostRoot`. */
  function run(args: RunRequest["args"], hostRoot = root, entry = "node_modules/fake/bin/fake.js") {
    return send(fileSystem, { run: { root: guest(hostRoot), entry, args } });
  }

  describe("when a parent of the root has the entry point", () => {
    const response = run(["--print-config", { temporary: "cache" }]);
    const { exited } = response as Extract<RunResponse, { exited: unknown }>;
    const report = JSON.parse(exited.stdout) as LinterReport;

    test("it runs the linter from the root and reports its exit code", () => {
      assert.partialDeepStrictEqual(response, { exited: { code: 3 } });
      assert.equal(report.cwd, root);
    });

    test("it passes literal arguments unchanged", () => {
      assert.equal(report.args[0], "--print-config");
    });

    test("it places temporary files in a directory that exists during the run", () => {
      assert.equal(path.basename(report.args[1] ?? ""), "cache");
      assert.equal(report.temporaryDirectoryExists, true);
    });

    test("it removes the temporary directory afterward", () => {
      assert.equal(fs.existsSync(path.dirname(report.args[1] ?? "")), false);
    });
  });

  describe("when the system temporary directory is missing", () => {
    /** Runs the fake linter while the system temporary directory does not exist. */
    function runWithoutTemporaryDirectory(args: Parameters<typeof run>[0]) {
      // `os.tmpdir()` reads `TMPDIR` on POSIX and `TEMP` or `TMP` on Windows.
      const names = ["TMPDIR", "TEMP", "TMP"] as const;
      const previous = names.map((name) => process.env[name]);
      for (const name of names) {
        process.env[name] = path.join(workspace, "missing-temporary");
      }
      try {
        return run(args);
      } finally {
        names.forEach((name, index) => {
          const value = previous[index];
          if (value === undefined) {
            delete process.env[name];
          } else {
            process.env[name] = value;
          }
        });
      }
    }

    describe("and the linter needs no temporary files", () => {
      test("it runs the linter without creating a temporary directory", () => {
        assert.partialDeepStrictEqual(runWithoutTemporaryDirectory([]), { exited: { code: 3 } });
      });
    });

    describe("and the linter needs a temporary file", () => {
      test("it reports the temporary-directory failure", () => {
        assert.partialDeepStrictEqual(runWithoutTemporaryDirectory([{ temporary: "cache" }]), {
          temporaryFailed: {},
        });
      });
    });
  });

  describe("when no parent of the root has the entry point", () => {
    test("it reports the missing entry point", () => {
      assert.equal(run([], root, "node_modules/missing/bin/missing.js"), "missingEntry");
    });
  });

  describe("when the root does not exist", () => {
    test("it reports that the linter could not start", () => {
      assert.partialDeepStrictEqual(run([], path.join(root, "missing")), { launchFailed: {} });
    });
  });

  describe("when the module asks for a guest path", () => {
    const windows = wasiFileSystem("win32", absolute("C:\\work"));

    describe("and the module can see the host path", () => {
      test("it returns the module's spelling", () => {
        assert.equal(send(windows, { guestPath: { path: "C:\\work\\a.ts" } }), "/c/work/a.ts");
      });
    });

    describe("and the module cannot see the host path", () => {
      test("it returns the host path unchanged", () => {
        assert.equal(send(windows, { guestPath: { path: "D:\\work\\a.ts" } }), "D:\\work\\a.ts");
      });
    });
  });
});
