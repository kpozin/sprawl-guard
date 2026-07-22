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

  describe("when a custom channel uses the staging namespace", () => {
    it("rejects the channel before invoking the publisher", () => {
      assert.deepEqual(
        resolveNpmChannelPublication([
          ...RELEASE_ARGUMENTS,
          "custom",
          "sg-candidate-user",
        ]),
        {
          kind: "invalid_channel",
          detail: { kind: "invalid_custom_tag", value: "sg-candidate-user" },
        },
      );
    });
  });
});
