import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { releaseNotes } from "./release-notes.ts";
import { trustedPublicPackageVersion } from "../package-version.ts";

const VERSION = trustedPublicPackageVersion("0.5.1");

describe("releaseNotes", () => {
  describe("when the version has a section", () => {
    const changelog = [
      "# Changelog",
      "",
      "## [Unreleased]",
      "",
      "## [0.5.10] - 2026-12-01",
      "",
      "- Not this one.",
      "",
      "## [0.5.1] - 2026-11-18",
      "",
      "A summary that wraps",
      "onto a second line.",
      "",
      "### Fixed",
      "",
      "- A fix that wraps",
      "  onto a second line.",
      "  - A nested detail that also",
      "    wraps.",
      "- Another fix.",
      "",
      "## [0.5.0] - 2026-11-17",
      "",
      "- Older.",
      "",
    ].join("\n");

    it("returns only that section, with each wrapped line joined to its paragraph or item", () => {
      assert.deepEqual(releaseNotes({ changelog, version: VERSION }), {
        kind: "ok",
        notes: [
          "A summary that wraps onto a second line.",
          "",
          "### Fixed",
          "",
          "- A fix that wraps onto a second line.",
          "  - A nested detail that also wraps.",
          "- Another fix.",
          "",
        ].join("\n"),
      });
    });
  });

  describe("when an entry has a fenced code example", () => {
    const changelog = [
      "## [0.5.1] - 2026-11-18",
      "",
      "- Run it like this:",
      "",
      "  ```sh",
      "  sprawl-guard check",
      "  ## not a heading",
      "  ```",
      "- A following entry that",
      "  wraps.",
      "",
      "## [0.5.0] - 2026-11-17",
      "",
      "- Older.",
      "",
    ].join("\n");

    it("keeps the code block's lines and the entries after it", () => {
      assert.deepEqual(releaseNotes({ changelog, version: VERSION }), {
        kind: "ok",
        notes: [
          "- Run it like this:",
          "",
          "  ```sh",
          "  sprawl-guard check",
          "  ## not a heading",
          "  ```",
          "- A following entry that wraps.",
          "",
        ].join("\n"),
      });
    });
  });

  describe("when a code example shows a shorter fence", () => {
    const changelog = [
      "## [0.5.1] - 2026-11-18",
      "",
      "- Document a fence like this:",
      "",
      "  ````md",
      "  ```sh",
      "  ## not a heading",
      "  ```",
      "  ````",
      "- A following entry.",
      "",
      "## [0.5.0] - 2026-11-17",
      "",
    ].join("\n");

    it("keeps the whole example inside the outer block", () => {
      assert.deepEqual(releaseNotes({ changelog, version: VERSION }), {
        kind: "ok",
        notes: `${changelog.split("\n").slice(2, 10).join("\n")}\n`,
      });
    });
  });

  describe("when the version's section has no entries", () => {
    it("refuses to return empty notes", () => {
      const changelog = "# Changelog\n\n## [0.5.1] - 2026-11-18\n\n### Fixed\n\n## [0.5.0]\n";

      assert.deepEqual(releaseNotes({ changelog, version: VERSION }), {
        kind: "empty_version_section",
        version: "0.5.1",
      });
    });
  });

  describe("when the version has no section", () => {
    it("reports it missing", () => {
      assert.deepEqual(releaseNotes({ changelog: "# Changelog\n", version: VERSION }), {
        kind: "missing_version_section",
        version: "0.5.1",
      });
    });
  });
});
