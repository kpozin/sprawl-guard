import assert from "node:assert/strict";
import path from "node:path";
import { describe, it } from "node:test";
import { RUNTIME_PACKAGE_DEFINITIONS } from "./package-manifests.ts";
import { stagedPayloadPackVerificationRequests } from "./verify-staged-payload-packs.ts";

describe("stagedPayloadPackVerificationRequests", () => {
  describe("when building verifier requests for the rendered publish workspace", () => {
    it("derives every runtime package and payload from shared metadata", () => {
      const requests = stagedPayloadPackVerificationRequests({
        workspaceRootDirectory: "/workspace",
      });

      assert.equal(requests.length, RUNTIME_PACKAGE_DEFINITIONS.length);
      assert.deepEqual(
        requests.map((request) => request.packageDirectory),
        RUNTIME_PACKAGE_DEFINITIONS.map((definition) =>
          path.join("/workspace", ".stage", "packages", definition.slug),
        ),
      );
      assert.deepEqual(
        requests.map((request) => request.requiredPaths),
        RUNTIME_PACKAGE_DEFINITIONS.map((definition) => [
          "README.md",
          definition.payloadPath,
          "LICENSE-MIT",
          "LICENSE-APACHE",
        ]),
      );
    });
  });
});
