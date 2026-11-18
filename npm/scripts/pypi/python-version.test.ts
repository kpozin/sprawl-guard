import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { trustedPublicPackageVersion } from "../package-version.ts";
import { pythonPackageVersion } from "./python-version.ts";

describe("pythonPackageVersion", () => {
  describe("with a stable release", () => {
    it("keeps the version as it is", () => {
      assert.deepEqual(pythonPackageVersion(trustedPublicPackageVersion("0.5.2")), {
        kind: "ok",
        version: "0.5.2",
      });
    });
  });

  describe("with a prerelease that PEP 440 can spell", () => {
    for (const [releaseVersion, version] of [
      ["0.5.2-alpha.3", "0.5.2a3"],
      ["0.5.2-beta.1", "0.5.2b1"],
      ["1.0.0-rc.10", "1.0.0rc10"],
      ["0.3.0-dev.0", "0.3.0.dev0"],
    ] as const) {
      it(`spells ${releaseVersion} as ${version}`, () => {
        assert.deepEqual(pythonPackageVersion(trustedPublicPackageVersion(releaseVersion)), {
          kind: "ok",
          version,
        });
      });
    }
  });

  describe("with a prerelease that PEP 440 cannot spell", () => {
    for (const releaseVersion of [
      "0.5.2-1",
      "0.5.2-beta",
      "0.5.2-beta.1.2",
      "0.5.2-canary.1",
      "0.5.2-beta.x",
      "0.5.2-constructor.1",
    ]) {
      it(`rejects ${releaseVersion}`, () => {
        assert.deepEqual(pythonPackageVersion(trustedPublicPackageVersion(releaseVersion)), {
          kind: "unsupported_prerelease",
          releaseVersion,
        });
      });
    }
  });
});
