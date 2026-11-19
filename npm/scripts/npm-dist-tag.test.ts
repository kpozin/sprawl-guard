import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { resolveNpmDistTagSelection } from "./npm-dist-tag.ts";
import type { PublicPackageVersion } from "./package-version.ts";

const STABLE_VERSION = "1.2.3" as PublicPackageVersion;

describe("resolveNpmDistTagSelection", () => {
  describe("when a predefined channel is selected", () => {
    it("returns the channel without a custom value", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({ customTag: "", releaseVersion: STABLE_VERSION, selection: "beta" }),
        { kind: "ok", tag: "beta" },
      );
    });

    it("rejects an ignored custom value", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({ customTag: "preview", releaseVersion: STABLE_VERSION, selection: "next" }),
        { kind: "unexpected_custom_tag", selection: "next" },
      );
    });
  });

  describe("when auto is selected", () => {
    function resolveAuto(version: string) {
      return resolveNpmDistTagSelection({
        customTag: "",
        releaseVersion: version as PublicPackageVersion,
        selection: "auto",
      });
    }

    describe("for a stable release", () => {
      it("picks latest", () => {
        assert.deepEqual(resolveAuto("1.2.3"), { kind: "ok", tag: "latest" });
      });
    });

    describe("for a beta or rc prerelease", () => {
      it("picks the channel that the prerelease label names", () => {
        assert.deepEqual(resolveAuto("0.6.0-beta.1"), { kind: "ok", tag: "beta" });
        assert.deepEqual(resolveAuto("1.0.0-rc.2"), { kind: "ok", tag: "rc" });
      });
    });

    describe("for a prerelease whose label names no channel", () => {
      it("asks for an explicit channel", () => {
        for (const version of ["1.0.0-alpha.1", "1.0.0-dev.3", "1.0.0-latest.1"]) {
          assert.deepEqual(resolveAuto(version), { kind: "no_automatic_channel", releaseVersion: version });
        }
      });
    });

    describe("with a custom value", () => {
      it("rejects the ignored custom value", () => {
        assert.deepEqual(
          resolveNpmDistTagSelection({ customTag: "preview", releaseVersion: STABLE_VERSION, selection: "auto" }),
          { kind: "unexpected_custom_tag", selection: "auto" },
        );
      });
    });
  });

  describe("when the custom channel is selected", () => {
    it("returns a validated custom tag", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({
          customTag: "internal-preview.2",
          releaseVersion: STABLE_VERSION,
          selection: "custom",
        }),
        { kind: "ok", tag: "internal-preview.2" },
      );
    });

    it("requires the custom value", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({ customTag: "", releaseVersion: STABLE_VERSION, selection: "custom" }),
        { kind: "missing_custom_tag" },
      );
    });

    it("rejects version-like values", () => {
      for (const customTag of ["2.0.0", "vnext"]) {
        assert.deepEqual(
          resolveNpmDistTagSelection({ customTag, releaseVersion: STABLE_VERSION, selection: "custom" }),
          { kind: "invalid_custom_tag", value: customTag },
        );
      }
    });
  });

  describe("when the selection is unknown", () => {
    it("rejects it exhaustively", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({ customTag: "", releaseVersion: STABLE_VERSION, selection: "stable" }),
        { kind: "invalid_selection", value: "stable" },
      );
    });
  });
});
