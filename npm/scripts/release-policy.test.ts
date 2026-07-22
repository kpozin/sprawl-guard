import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { NPM_DIST_TAG_SCHEMA } from "./npm-dist-tag.ts";
import {
  classifyGitHubRelease,
  expectedReleaseAssetNames,
  parseReleaseTag,
  validateReleaseVersions,
  type ReleaseVersion,
} from "./release-policy.ts";

function validReleaseVersion(value: string): ReleaseVersion {
  const parsed = parseReleaseTag(`v${value}`);
  assert.equal(parsed.kind, "ok");
  return parsed.releaseVersion;
}

describe("parseReleaseTag", () => {
  describe("when the tag contains a publishable SemVer version", () => {
    it("returns branded release metadata", () => {
      assert.deepEqual(parseReleaseTag("v1.2.3-rc.1"), {
        kind: "ok",
        releaseRef: "refs/tags/v1.2.3-rc.1",
        releaseTag: "v1.2.3-rc.1",
        releaseVersion: "1.2.3-rc.1",
      });
    });
  });

  describe("when the tag does not contain a publishable version", () => {
    it("rejects leading zeroes and malformed identifiers", () => {
      for (const releaseTag of [
        "v01.2.3",
        "v1.2.3-.",
        "v1.2.3+build.1",
        "v1.2.3+meta+again",
        "1.2.3",
      ]) {
        assert.deepEqual(parseReleaseTag(releaseTag), {
          kind: "invalid_release_tag",
          releaseTag,
        });
      }
    });
  });
});

describe("validateReleaseVersions", () => {
  describe("when every published package matches the tag", () => {
    it("accepts the release", () => {
      const releaseVersion = validReleaseVersion("1.2.3");
      assert.deepEqual(
        validateReleaseVersions({
          cargoPackages: [
            { name: "sprawl-guard", version: releaseVersion },
            { name: "sprawl-guard-lib", version: releaseVersion },
          ],
          npmVersion: releaseVersion,
          releaseVersion,
        }),
        { kind: "ok" },
      );
    });
  });

  describe("when a published package differs from the tag", () => {
    it("reports every missing or mismatched package", () => {
      const releaseVersion = validReleaseVersion("1.2.3");
      assert.deepEqual(
        validateReleaseVersions({
          cargoPackages: [
            {
              name: "sprawl-guard",
              version: validReleaseVersion("1.2.2"),
            },
          ],
          npmVersion: validReleaseVersion("1.2.1"),
          releaseVersion,
        }),
        {
          kind: "version_mismatch",
          mismatches: [
            {
              actualVersion: "1.2.2",
              packageName: "sprawl-guard",
            },
            {
              actualVersion: null,
              packageName: "sprawl-guard-lib",
            },
            {
              actualVersion: "1.2.1",
              packageName: "@kpozin/sprawl-guard",
            },
          ],
          releaseVersion: "1.2.3",
        },
      );
    });
  });
});

describe("classifyGitHubRelease", () => {
  describe("when the version has a SemVer prerelease component", () => {
    it("classifies the GitHub Release as a prerelease for every npm channel", () => {
      assert.deepEqual(
        classifyGitHubRelease(
          validReleaseVersion("1.2.3-rc.1"),
          NPM_DIST_TAG_SCHEMA.parse("rc"),
        ),
        { kind: "prerelease" },
      );
    });
  });

  describe("when the version is stable", () => {
    it("classifies only the npm latest channel as GitHub latest", () => {
      assert.deepEqual(
        classifyGitHubRelease(
          validReleaseVersion("1.2.3"),
          NPM_DIST_TAG_SCHEMA.parse("latest"),
        ),
        { kind: "stable_latest" },
      );
      assert.deepEqual(
        classifyGitHubRelease(
          validReleaseVersion("1.2.3"),
          NPM_DIST_TAG_SCHEMA.parse("next"),
        ),
        { kind: "stable_non_latest" },
      );
    });
  });
});

describe("expectedReleaseAssetNames", () => {
  describe("when rendering the asset contract", () => {
    it("uses ZIP for Windows and tarballs for every other target", () => {
      const names = expectedReleaseAssetNames(validReleaseVersion("1.2.3"));

      assert.ok(
        names.some(
          (name) => name === "sprawl-guard-1.2.3-x86_64-pc-windows-msvc.zip",
        ),
      );
      assert.ok(
        names.some(
          (name) => name === "sprawl-guard-1.2.3-aarch64-apple-darwin.tar.gz",
        ),
      );
      assert.ok(
        names.some((name) => name === "sprawl-guard-1.2.3-checksums.txt"),
      );
      assert.equal(names.length, 8);
    });
  });
});
