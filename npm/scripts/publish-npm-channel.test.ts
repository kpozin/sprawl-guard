import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { resolveNpmChannelPublication } from "./publish-npm-channel.ts";

const RELEASE_ARGUMENTS = [
  "/release/npm-candidate",
  "0123456789abcdef0123456789abcdef01234567",
  "v0.1.0",
] as const;

describe("resolveNpmChannelPublication", () => {
  describe("when a preset channel is selected", () => {
    it("forwards the validated channel to the low-level publisher", () => {
      const result = resolveNpmChannelPublication([
        ...RELEASE_ARGUMENTS,
        "beta",
      ]);

      assert.equal(result.kind, "ok");
      if (result.kind === "ok") {
        assert.deepEqual(result.childArguments.slice(1), [
          "publish",
          ...RELEASE_ARGUMENTS,
          "beta",
        ]);
      }
    });
  });

  describe("when auto is selected", () => {
    it("derives the channel from the release tag's version", () => {
      const result = resolveNpmChannelPublication([
        "/release/npm-candidate",
        "0123456789abcdef0123456789abcdef01234567",
        "v0.6.0-beta.1",
        "auto",
      ]);

      assert.equal(result.kind, "ok");
      if (result.kind === "ok") {
        assert.equal(result.childArguments.at(-1), "beta");
      }
    });
  });

  describe("when the release tag holds no publishable version", () => {
    it("rejects the tag before invoking the publisher", () => {
      assert.deepEqual(
        resolveNpmChannelPublication([
          "/release/npm-candidate",
          "0123456789abcdef0123456789abcdef01234567",
          "release-1",
          "latest",
        ]),
        { kind: "invalid_release_tag", releaseTag: "release-1" },
      );
    });
  });

  describe("when a custom channel is version-like", () => {
    it("rejects the channel before invoking the publisher", () => {
      assert.deepEqual(
        resolveNpmChannelPublication([
          ...RELEASE_ARGUMENTS,
          "custom",
          "2.0.0",
        ]),
        {
          kind: "invalid_channel",
          detail: { kind: "invalid_custom_tag", value: "2.0.0" },
        },
      );
    });
  });
});
