import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { dateChangelog, RELEASE_DATE_SCHEMA } from "./date-changelog.ts";
import { trustedPublicPackageVersion } from "../package-version.ts";

const VERSION = trustedPublicPackageVersion("0.5.1");
const DATE = RELEASE_DATE_SCHEMA.parse("2026-11-18");

describe("dateChangelog", () => {
  describe("when the unreleased section lists changes", () => {
    const changelog = [
      "# Changelog",
      "",
      "## [Unreleased]",
      "",
      "### Fixed",
      "",
      "- A fix.",
      "",
      "## [0.5.0] - 2026-11-17",
      "",
      "- Older.",
      "",
    ].join("\n");

    it("moves them under a dated heading for the version below a new empty section", () => {
      assert.deepEqual(dateChangelog({ changelog, version: VERSION, date: DATE }), {
        kind: "ok",
        changelog: [
          "# Changelog",
          "",
          "## [Unreleased]",
          "",
          "## [0.5.1] - 2026-11-18",
          "",
          "### Fixed",
          "",
          "- A fix.",
          "",
          "## [0.5.0] - 2026-11-17",
          "",
          "- Older.",
          "",
        ].join("\n"),
      });
    });

    describe("and the version is not after the latest release", () => {
      for (const version of ["0.5.0", "0.4.2", "0.5.0-rc.1"]) {
        it(`refuses ${version}`, () => {
          assert.deepEqual(
            dateChangelog({ changelog, version: trustedPublicPackageVersion(version), date: DATE }),
            { kind: "version_not_newer", version, latest: "0.5.0" },
          );
        });
      }
    });

    describe("and the version is a prerelease after the latest release", () => {
      it("dates it", () => {
        const version = trustedPublicPackageVersion("0.6.0-rc.1");

        assert.equal(dateChangelog({ changelog, version, date: DATE }).kind, "ok");
      });
    });
  });

  describe("when the unreleased section is empty", () => {
    it("refuses to date a release with no notes", () => {
      const changelog = "# Changelog\n\n## [Unreleased]\n\n## [0.5.0] - 2026-11-17\n\n- Older.\n";

      assert.deepEqual(dateChangelog({ changelog, version: VERSION, date: DATE }), {
        kind: "empty_unreleased_section",
      });
    });

    describe("except for subsection headings", () => {
      it("refuses to date a release with no notes", () => {
        const changelog = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n\n## [0.5.0] - 2026-11-17\n";

        assert.deepEqual(dateChangelog({ changelog, version: VERSION, date: DATE }), {
          kind: "empty_unreleased_section",
        });
      });
    });
  });

  describe("when the changelog has no unreleased section", () => {
    it("reports it missing", () => {
      assert.deepEqual(
        dateChangelog({ changelog: "# Changelog\n", version: VERSION, date: DATE }),
        { kind: "missing_unreleased_section" },
      );
    });
  });
});
