import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { dateChangelog, RELEASE_DATE_SCHEMA, type ReleaseStage } from "./date-changelog.ts";
import { trustedPublicPackageVersion } from "../package-version.ts";

const VERSION = trustedPublicPackageVersion("0.5.1");
/** Preparation before setting the version, with the latest changelog heading's version. */
const STAGE: ReleaseStage = {
  kind: "before_set_version",
  packageVersion: trustedPublicPackageVersion("0.5.0"),
};
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
      assert.deepEqual(
        dateChangelog({ changelog, version: VERSION, stage: STAGE, date: DATE }),
        {
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
        },
      );
    });

    describe("and the version is not after the latest release", () => {
      for (const version of ["0.5.0", "0.4.2", "0.5.0-rc.1"]) {
        it(`refuses ${version}`, () => {
          assert.deepEqual(
            dateChangelog({
              changelog,
              version: trustedPublicPackageVersion(version),
              stage: STAGE,
              date: DATE,
            }),
            { kind: "version_not_newer", version, latest: "0.5.0" },
          );
        });
      }
    });

    describe("and the version is a prerelease after the latest release", () => {
      it("dates it", () => {
        const version = trustedPublicPackageVersion("0.6.0-rc.1");

        assert.equal(
          dateChangelog({ changelog, version, stage: STAGE, date: DATE }).kind,
          "ok",
        );
      });
    });

    describe("and the package version is a prerelease whose section was folded away", () => {
      const packageVersion = trustedPublicPackageVersion("0.6.0-beta.3");
      const stage: ReleaseStage = { kind: "before_set_version", packageVersion };

      describe("and the version isn't after the package version", () => {
        for (const version of ["0.5.1", "0.6.0-beta.3"]) {
          it(`refuses ${version}`, () => {
            assert.deepEqual(
              dateChangelog({
                changelog,
                version: trustedPublicPackageVersion(version),
                stage,
                date: DATE,
              }),
              { kind: "version_not_after_package", version, packageVersion: "0.6.0-beta.3" },
            );
          });
        }
      });

      describe("and the version is after the package version", () => {
        it("dates it", () => {
          const version = trustedPublicPackageVersion("0.6.0");

          assert.equal(dateChangelog({ changelog, version, stage, date: DATE }).kind, "ok");
        });
      });
    });

    describe("and the package already has the version", () => {
      it("dates it", () => {
        assert.equal(
          dateChangelog({
            changelog,
            version: VERSION,
            stage: { kind: "after_set_version" },
            date: DATE,
          }).kind,
          "ok",
        );
      });
    });
  });

  describe("when the changelog has prerelease sections", () => {
    const changelog = [
      "# Changelog",
      "",
      "## [Unreleased]",
      "",
      "- A fix.",
      "",
      "## [0.6.0-beta.2] - 2026-11-19",
      "",
      "- Newer beta.",
      "",
      "## [0.6.0-beta.1] - 2026-11-18",
      "",
      "- Older beta.",
      "",
      "## [0.5.0] - 2026-11-17",
      "",
      "- Older.",
      "",
    ].join("\n");
    const stage: ReleaseStage = {
      kind: "before_set_version",
      packageVersion: trustedPublicPackageVersion("0.6.0-beta.2"),
    };

    describe("and the version is stable", () => {
      it("refuses until their notes are folded into the unreleased section", () => {
        const version = trustedPublicPackageVersion("0.6.0");

        assert.deepEqual(dateChangelog({ changelog, version, stage, date: DATE }), {
          kind: "prerelease_sections_present",
          prereleases: ["0.6.0-beta.2", "0.6.0-beta.1"],
        });
      });
    });

    describe("and the version is another prerelease", () => {
      it("dates it", () => {
        const version = trustedPublicPackageVersion("0.6.0-beta.3");

        assert.equal(dateChangelog({ changelog, version, stage, date: DATE }).kind, "ok");
      });
    });
  });

  describe("when the unreleased section is empty", () => {
    it("refuses to date a release with no notes", () => {
      const changelog = "# Changelog\n\n## [Unreleased]\n\n## [0.5.0] - 2026-11-17\n\n- Older.\n";

      assert.deepEqual(
        dateChangelog({ changelog, version: VERSION, stage: STAGE, date: DATE }),
        { kind: "empty_unreleased_section" },
      );
    });

    describe("except for subsection headings", () => {
      it("refuses to date a release with no notes", () => {
        const changelog = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n\n## [0.5.0] - 2026-11-17\n";

        assert.deepEqual(
          dateChangelog({ changelog, version: VERSION, stage: STAGE, date: DATE }),
          { kind: "empty_unreleased_section" },
        );
      });
    });
  });

  describe("when the changelog has no unreleased section", () => {
    it("reports it missing", () => {
      assert.deepEqual(
        dateChangelog({
          changelog: "# Changelog\n",
          version: VERSION,
          stage: STAGE,
          date: DATE,
        }),
        { kind: "missing_unreleased_section" },
      );
    });
  });
});
