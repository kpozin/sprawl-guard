import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { checkCli, type CommandOutput } from "./check-cli.ts";
import { PACKAGE_VERSION } from "../package-manifests.ts";
import { trustedPublicPackageVersion } from "../package-version.ts";

const SCRIPT_PATH = fileURLToPath(new URL("./check-cli.ts", import.meta.url));
const VERSION = trustedPublicPackageVersion("1.2.3");
const TEMPORARY_ROOT = mkdtempSync(path.join(tmpdir(), "check-cli-"));

after(() => {
  rmSync(TEMPORARY_ROOT, { force: true, recursive: true });
});

/** Returns a command runner that answers `--version` and `--help` and records each call. */
function fakeCli(versionOutput: CommandOutput) {
  const calls: (readonly string[])[] = [];
  return {
    calls,
    run(args: readonly string[]): CommandOutput {
      calls.push(args);
      return args.at(-1) === "--version"
        ? versionOutput
        : { kind: "ok", stderr: "", stdout: "usage" };
    },
  };
}

describe("checkCli", () => {
  describe("when the command prints exactly the version", () => {
    it("also runs its help", () => {
      const cli = fakeCli({ kind: "ok", stderr: "", stdout: `sprawl-guard ${VERSION}\n` });

      const result = checkCli(["node", "bin.js"], VERSION, cli.run);

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(cli.calls, [
        ["node", "bin.js", "--version"],
        ["node", "bin.js", "--help"],
      ]);
    });
  });

  describe("when the command prints another version", () => {
    it("reports the output without running help", () => {
      const cli = fakeCli({ kind: "ok", stderr: "", stdout: "sprawl-guard 1.2.2\n" });

      const result = checkCli(["sprawl-guard"], VERSION, cli.run);

      assert.deepEqual(result, {
        kind: "unexpected_version_output",
        stderr: "",
        stdout: "sprawl-guard 1.2.2\n",
      });
      assert.equal(cli.calls.length, 1);
    });
  });

  describe("when the command writes to stderr", () => {
    it("reports the output", () => {
      const cli = fakeCli({
        kind: "ok",
        stderr: "warning\n",
        stdout: `sprawl-guard ${VERSION}\n`,
      });

      assert.equal(checkCli(["sprawl-guard"], VERSION, cli.run).kind, "unexpected_version_output");
    });
  });

  describe("when the command fails", () => {
    it("reports the failure", () => {
      const cli = fakeCli({ kind: "failed", detail: "exited with 1" });

      assert.deepEqual(checkCli(["sprawl-guard"], VERSION, cli.run), {
        kind: "command_failed",
        args: ["sprawl-guard", "--version"],
        detail: "exited with 1",
      });
    });
  });
});

describe("check-cli.ts", () => {
  /** Writes a CLI that prints the given version and returns the script's exit status. */
  function runScript(printedVersion: string): ReturnType<typeof spawnSync> {
    const binPath = path.join(mkdtempSync(path.join(TEMPORARY_ROOT, "bin-")), "cli.js");
    writeFileSync(
      binPath,
      `process.stdout.write(process.argv[2] === "--version" ? "sprawl-guard ${printedVersion}\\n" : "usage\\n");\n`,
    );
    return spawnSync(process.execPath, [SCRIPT_PATH, "--", process.execPath, binPath], {
      encoding: "utf8",
    });
  }

  describe("when the command prints the public package version", () => {
    it("succeeds", () => {
      const result = runScript(PACKAGE_VERSION);

      assert.equal(result.status, 0, String(result.stderr));
    });
  });

  describe("when the command prints another version", () => {
    it("fails and shows what it printed", () => {
      const result = runScript("0.0.0-other");

      assert.equal(result.status, 1);
      assert.match(String(result.stderr), /printed "sprawl-guard 0\.0\.0-other\\n"/u);
    });
  });

  describe("without a command", () => {
    it("prints usage", () => {
      const result = spawnSync(process.execPath, [SCRIPT_PATH], { encoding: "utf8" });

      assert.equal(result.status, 2);
      assert.match(result.stderr, /usage: check-cli\.ts -- COMMAND/u);
    });
  });
});
