import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { checkDirectoryLinksReport, REPOSITORY_SCAN } from "./check-directory-links.ts";

const OTHER_UNREADABLE = { kind: "permission_denied", path: "other", detail: "denied" };

/** Returns a `check --format json` report with the given violation and loop paths. */
function report(violations: readonly string[], loops: readonly string[]): string {
  return JSON.stringify({
    violations: violations.map((path) => ({ path })),
    coverage: {
      unreadable_paths: [
        ...loops.map((path) => ({ kind: "symlink_loop", path, detail: "symlink loop found" })),
        OTHER_UNREADABLE,
      ],
    },
  });
}

describe("checkDirectoryLinksReport", () => {
  describe("when the report names both routes to the file and the loop", () => {
    it("passes, whatever order the scan used", () => {
      const result = checkDirectoryLinksReport(
        report([...REPOSITORY_SCAN.violations].reverse(), [...REPOSITORY_SCAN.loops].reverse()),
        REPOSITORY_SCAN,
      );

      assert.deepEqual(result, { kind: "ok" });
    });
  });

  describe("when the scan did not follow the link", () => {
    it("reports the paths it found and why others were unreadable", () => {
      const result = checkDirectoryLinksReport(report(["real/lib.rs"], []), REPOSITORY_SCAN);

      assert.deepEqual(result, {
        kind: "unexpected_paths",
        found: { violations: ["real/lib.rs"], loops: [] },
        unreadable: [OTHER_UNREADABLE],
      });
    });
  });

  describe("when the output is not a report", () => {
    it("says the report is invalid", () => {
      const result = checkDirectoryLinksReport("{}", REPOSITORY_SCAN);

      assert.equal(result.kind, "invalid_report");
    });
  });
});
