import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { checkLanguagesReport } from "./check-languages-report.ts";

const SCRIPT_PATH = fileURLToPath(new URL("./check-languages-report.ts", import.meta.url));
const TEMPORARY_ROOT = mkdtempSync(path.join(tmpdir(), "check-languages-report-"));

after(() => {
  rmSync(TEMPORARY_ROOT, { force: true, recursive: true });
});

/** Serializes a `languages --detected --format json` report. */
function report(mode: string, languages: readonly object[]): string {
  return JSON.stringify({ languages, mode });
}

describe("checkLanguagesReport", () => {
  describe("when the detected report counts sources for the language", () => {
    it("accepts it", () => {
      const text = report("detected", [
        { config_key: "markdown", source_files: 3 },
        { config_key: "rust", source_files: 12 },
      ]);

      assert.deepEqual(checkLanguagesReport(text, "rust"), { kind: "ok" });
    });
  });

  describe("when the language has no source files", () => {
    it("reports the missing language", () => {
      const text = report("detected", [{ config_key: "rust", source_files: 0 }]);

      assert.deepEqual(checkLanguagesReport(text, "rust"), {
        kind: "language_missing",
        language: "rust",
      });
    });
  });

  describe("when the report lists every language rather than detected ones", () => {
    it("reports the mode", () => {
      const text = report("all", [{ config_key: "rust", source_files: 12 }]);

      assert.deepEqual(checkLanguagesReport(text, "rust"), {
        kind: "unexpected_mode",
        mode: "all",
      });
    });
  });

  describe("when the report has another shape", () => {
    it("reports invalid JSON", () => {
      assert.equal(checkLanguagesReport('{"mode":"detected"}', "rust").kind, "invalid_report");
      assert.equal(checkLanguagesReport("not json", "rust").kind, "invalid_report");
    });
  });
});

describe("check-languages-report.ts", () => {
  /** Writes a report and runs the script on it. */
  function runScript(text: string, language: string): ReturnType<typeof spawnSync> {
    const reportPath = path.join(mkdtempSync(path.join(TEMPORARY_ROOT, "case-")), "report.json");
    writeFileSync(reportPath, text);
    return spawnSync(
      process.execPath,
      [SCRIPT_PATH, "--report", reportPath, "--language", language],
      { encoding: "utf8" },
    );
  }

  describe("when the report counts sources for the language", () => {
    it("succeeds", () => {
      const result = runScript(report("detected", [{ config_key: "typescript", source_files: 4 }]), "typescript");

      assert.equal(result.status, 0, String(result.stderr));
    });
  });

  describe("when the language is missing", () => {
    it("fails with a reason", () => {
      const result = runScript(report("detected", []), "typescript");

      assert.equal(result.status, 1);
      assert.match(String(result.stderr), /found no typescript source files/u);
    });
  });
});
