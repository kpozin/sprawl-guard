import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { resolveNpmDistTagSelection } from "./npm-dist-tag.ts";

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

    it("rejects version-like values", () => {
      for (const customTag of ["2.0.0", "vnext"]) {
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
