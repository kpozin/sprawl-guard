import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import {
  parsePackDryRunReport,
  parseVerifierArgs,
  pnpmPackDryRunCommand,
  verifyPackedFiles,
  type PackDryRunReport,
  type PackVerifierArgs,
} from "./verify-packed-files.ts";

const verifierScriptPath = fileURLToPath(new URL("./verify-packed-files.ts", import.meta.url));

describe("parseVerifierArgs", () => {
  describe("when explicit package-content rules are provided", () => {
    it("returns required-path and forbidden-suffix rules", () => {
      const result = parseVerifierArgs([
        "./sprawl-guard",
        "--require",
        "dist/bin/sprawl-guard.js",
        "--forbid-suffix",
        ".test.js",
      ]);

      assert.equal(result.kind, "ok");
      assert.deepEqual((result as { value: PackVerifierArgs }).value, {
        packageDirectory: "./sprawl-guard",
        rules: [
          {
            kind: "required_path",
            path: "dist/bin/sprawl-guard.js",
          },
          {
            kind: "forbidden_suffix",
            suffix: ".test.js",
          },
        ],
      });
    });
  });

  describe("when a rule path is not package-relative", () => {
    it("rejects the arguments", () => {
      const result = parseVerifierArgs(["./sprawl-guard", "--require", "../secret"]);

      assert.equal(result.kind, "invalid_arguments");
    });
  });

  describe("when no package-content rules are provided", () => {
    it("rejects the arguments", () => {
      const result = parseVerifierArgs(["./sprawl-guard"]);

      assert.equal(result.kind, "invalid_arguments");
    });
  });
});

describe("verify-packed-files CLI", () => {
  describe("when the packed package satisfies every rule", () => {
    it("exits successfully", () => {
      const packageDirectory = createPackageFixture({
        files: ["dist/bin/sprawl-guard.js", "dist/lib/run-cli.js"],
        includeHijackingPnpmShim: true,
      });

      const result = runVerifier(
        [
          packageDirectory,
          "--require",
          "dist/bin/sprawl-guard.js",
          "--forbid-suffix",
          ".test.js",
        ],
        {
          ...process.env,
          PNPM_CONFIG_PM_ON_FAIL: "download",
        },
      );

      assert.equal(result.status, 0, result.stderr);
    });
  });

  describe("when a required file is absent from the packed package", () => {
    it("exits with a verification failure", () => {
      const packageDirectory = createPackageFixture({
        files: ["dist/lib/run-cli.js"],
      });

      const result = runVerifier([
        packageDirectory,
        "--require",
        "dist/bin/sprawl-guard.js",
      ]);

      assert.equal(result.status, 1);
      assert.match(result.stderr, /missing required file: dist\/bin\/sprawl-guard\.js/u);
    });
  });

  describe("when a forbidden suffix is present in the packed package", () => {
    it("exits with a verification failure", () => {
      const packageDirectory = createPackageFixture({
        files: ["dist/bin/sprawl-guard.js", "dist/lib/machine-request.test.js"],
      });

      const result = runVerifier([
        packageDirectory,
        "--require",
        "dist/bin/sprawl-guard.js",
        "--forbid-suffix",
        ".test.js",
      ]);

      assert.equal(result.status, 1);
      assert.match(
        result.stderr,
        /forbidden file matched suffix \.test\.js: dist\/lib\/machine-request\.test\.js/u,
      );
    });
  });
});

describe("verifyPackedFiles", () => {
  describe("when all required files are present", () => {
    describe("and no forbidden suffix matches", () => {
      it("accepts the packed file list", () => {
        const result = verifyPackedFiles(
          packReportWithFiles(["dist/bin/sprawl-guard.js", "dist/lib/run-cli.js"]),
          okVerifierArgs().rules,
        );

        assert.deepEqual(result, { kind: "ok" });
      });
    });
  });

  describe("when a required file is missing", () => {
    it("reports the missing file", () => {
      const result = verifyPackedFiles(
        packReportWithFiles(["dist/lib/run-cli.js"]),
        okVerifierArgs().rules,
      );

      assert.deepEqual(result, {
        kind: "violations",
        violations: [
          {
            kind: "missing_required_path",
            path: "dist/bin/sprawl-guard.js",
          },
        ],
      });
    });
  });

  describe("when a forbidden suffix is present", () => {
    it("reports the forbidden file", () => {
      const result = verifyPackedFiles(
        packReportWithFiles([
          "dist/bin/sprawl-guard.js",
          "dist/lib/machine-request.test.js",
        ]),
        okVerifierArgs().rules,
      );

      assert.deepEqual(result, {
        kind: "violations",
        violations: [
          {
            kind: "forbidden_path",
            path: "dist/lib/machine-request.test.js",
            suffix: ".test.js",
          },
        ],
      });
    });
  });
});

describe("parsePackDryRunReport", () => {
  describe("when pnpm reports an invalid package path", () => {
    it("rejects the report", () => {
      const result = parsePackDryRunReport(
        JSON.stringify({
          files: [
            {
              path: "../dist/bin/sprawl-guard.js",
            },
          ],
        }),
      );

      assert.equal(result.kind, "invalid_pack_report");
    });
  });
});

describe("pnpmPackDryRunCommand", () => {
  describe("when pnpm is a JavaScript launcher", () => {
    it("runs the absolute launcher through Node", () => {
      assert.deepEqual(
        pnpmPackDryRunCommand({
          nodeExecutablePath: "C:\\Program Files\\nodejs\\node.exe",
          packageManagerUserAgent: "pnpm/11.7.0 npm/? node/v24.0.0 win32 x64",
          platform: "win32",
          pnpmLauncherPath: "C:\\pnpm home\\pnpm.cjs",
        }),
        {
          kind: "ok",
          command: {
            kind: "javascript_launcher",
            executable: "C:\\Program Files\\nodejs\\node.exe",
            launcher: "C:\\pnpm home\\pnpm.cjs",
          },
        },
      );
    });
  });

  describe("when pnpm is a standalone Windows executable", () => {
    it("runs the absolute executable directly", () => {
      assert.deepEqual(
        pnpmPackDryRunCommand({
          nodeExecutablePath: "C:\\Program Files\\nodejs\\node.exe",
          packageManagerUserAgent: "pnpm/11.7.0 npm/? node/v24.0.0 win32 x64",
          platform: "win32",
          pnpmLauncherPath: "C:\\pnpm home\\pnpm.exe",
        }),
        {
          kind: "ok",
          command: {
            kind: "native_launcher",
            executable: "C:\\pnpm home\\pnpm.exe",
          },
        },
      );
    });
  });

  describe("when pnpm lifecycle provenance is absent", () => {
    it("rejects direct invocation", () => {
      assert.deepEqual(
        pnpmPackDryRunCommand({
          nodeExecutablePath: "/usr/bin/node",
          packageManagerUserAgent: undefined,
          platform: "linux",
          pnpmLauncherPath: undefined,
        }),
        {
          kind: "invalid_launcher",
          message: "npm_config_user_agent did not identify pnpm",
        },
      );
    });
  });

  describe("when the launcher path is relative", () => {
    it("rejects the untrusted launcher", () => {
      assert.deepEqual(
        pnpmPackDryRunCommand({
          nodeExecutablePath: "/usr/bin/node",
          packageManagerUserAgent: "pnpm/11.7.0 npm/? node/v24.0.0 linux x64",
          platform: "linux",
          pnpmLauncherPath: "pnpm",
        }),
        {
          kind: "invalid_launcher",
          message: "pnpm launcher path must be absolute: pnpm",
        },
      );
    });
  });

  describe("when an absolute launcher does not belong to pnpm", () => {
    it("rejects the launcher before execution", () => {
      assert.deepEqual(
        pnpmPackDryRunCommand({
          nodeExecutablePath: "/usr/bin/node",
          packageManagerUserAgent: "pnpm/11.7.0 npm/? node/v24.0.0 linux x64",
          platform: "linux",
          pnpmLauncherPath: "/usr/bin/npm-cli.js",
        }),
        {
          kind: "invalid_launcher",
          message: "unsupported pnpm launcher: /usr/bin/npm-cli.js",
        },
      );
    });
  });

  describe("when pnpm is a POSIX executable", () => {
    it("runs the absolute executable directly", () => {
      assert.deepEqual(
        pnpmPackDryRunCommand({
          nodeExecutablePath: "/usr/bin/node",
          packageManagerUserAgent: "pnpm/11.7.0 npm/? node/v24.0.0 linux x64",
          platform: "linux",
          pnpmLauncherPath: "/opt/pnpm/bin/pnpm",
        }),
        {
          kind: "ok",
          command: {
            kind: "native_launcher",
            executable: "/opt/pnpm/bin/pnpm",
          },
        },
      );
    });
  });
});

function okVerifierArgs(): PackVerifierArgs {
  const result = parseVerifierArgs([
    "./sprawl-guard",
    "--require",
    "dist/bin/sprawl-guard.js",
    "--forbid-suffix",
    ".test.js",
  ]);

  assert.equal(result.kind, "ok");
  return (result as { value: PackVerifierArgs }).value;
}

function packReportWithFiles(paths: readonly string[]): PackDryRunReport {
  const result = parsePackDryRunReport(
    JSON.stringify({
      files: paths.map((path) => ({ path })),
    }),
  );

  assert.equal(result.kind, "ok");
  return (result as { report: PackDryRunReport }).report;
}

function runVerifier(args: readonly string[], environment = process.env): {
  readonly status: number | null;
  readonly stdout: string;
  readonly stderr: string;
} {
  return spawnSync(process.execPath, [verifierScriptPath, ...args], {
    encoding: "utf8",
    env: environment,
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function createPackageFixture(args: {
  readonly files: readonly string[];
  readonly includeHijackingPnpmShim?: boolean;
}): string {
  const packageDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "sprawl guard pack "));
  fs.writeFileSync(
    path.join(packageDirectory, "package.json"),
    `${JSON.stringify(
      {
        name: "sprawl-guard-pack-fixture",
        packageManager: "pnpm@1.0.0",
        version: "0.0.0",
        files: ["dist/"],
      },
      null,
      2,
    )}\n`,
  );

  for (const relativePath of args.files) {
    const fullPath = path.join(packageDirectory, relativePath);
    fs.mkdirSync(path.dirname(fullPath), { recursive: true });
    fs.writeFileSync(fullPath, "");
  }

  if (args.includeHijackingPnpmShim === true) {
    fs.writeFileSync(
      path.join(packageDirectory, "pnpm.cmd"),
      "@echo off\r\necho package-local pnpm.cmd was executed 1>&2\r\nexit /b 97\r\n",
    );
  }

  return packageDirectory;
}
