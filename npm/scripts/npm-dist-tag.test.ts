import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  resolveNpmDistTagSelection,
  SELECTED_NPM_DIST_TAG_SCHEMA,
  STAGING_NPM_DIST_TAG_SCHEMA,
} from "./npm-dist-tag.ts";

describe("SELECTED_NPM_DIST_TAG_SCHEMA", () => {
  describe("when a local command receives an internal staging tag", () => {
    it("rejects the tag before candidate assembly or publication", () => {
      assert.equal(
        SELECTED_NPM_DIST_TAG_SCHEMA.safeParse("sg-candidate-user").success,
        false,
      );
    });
  });

  describe("when a selected channel is parsed as an internal staging tag", () => {
    it("requires the reserved prefix followed by one full lowercase commit", () => {
      for (const tag of [
        "beta",
        "sg-candidate-user",
        "sg-candidate-01234567",
        "sg-candidate-0123456789ABCDEF0123456789ABCDEF01234567",
      ]) {
        assert.equal(STAGING_NPM_DIST_TAG_SCHEMA.safeParse(tag).success, false);
      }
      assert.equal(
        STAGING_NPM_DIST_TAG_SCHEMA.safeParse(
          "sg-candidate-0123456789abcdef0123456789abcdef01234567",
        ).success,
        true,
      );
    });
  });
});

describe("resolveNpmDistTagSelection", () => {
  describe("when a predefined channel is selected", () => {
    it("returns the channel without a custom value", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({ customTag: "", selection: "beta" }),
        { kind: "ok", tag: "beta" },
      );
    });

    it("rejects an ignored custom value", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({ customTag: "preview", selection: "next" }),
        { kind: "unexpected_custom_tag", selection: "next" },
      );
    });
  });

  describe("when the custom channel is selected", () => {
    it("returns a validated custom tag", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({
          customTag: "internal-preview.2",
          selection: "custom",
        }),
        { kind: "ok", tag: "internal-preview.2" },
      );
    });

    it("requires the custom value", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({ customTag: "", selection: "custom" }),
        { kind: "missing_custom_tag" },
      );
    });

    it("rejects version-like values and the reserved staging namespace", () => {
      for (const customTag of ["2.0.0", "vnext", "sg-candidate-user"]) {
        assert.deepEqual(
          resolveNpmDistTagSelection({ customTag, selection: "custom" }),
          { kind: "invalid_custom_tag", value: customTag },
        );
      }
    });
  });

  describe("when the selection is unknown", () => {
    it("rejects it exhaustively", () => {
      assert.deepEqual(
        resolveNpmDistTagSelection({ customTag: "", selection: "stable" }),
        { kind: "invalid_selection", value: "stable" },
      );
    });
  });
});
