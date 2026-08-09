import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { PACKAGE_VERSION } from "./package-manifests.ts";
import { resolveReleaseMetadata } from "./release-metadata.ts";
import { RELEASE_COMMIT_SCHEMA } from "./release-source.ts";

const MATCHING_CARGO_METADATA = {
  packages: [
    { name: "sprawl-guard", version: PACKAGE_VERSION },
    { name: "sprawl-guard-lib", version: PACKAGE_VERSION },
  ],
};
const RELEASE_TAG = `v${PACKAGE_VERSION}`;
const RELEASE_COMMIT = RELEASE_COMMIT_SCHEMA.parse(
  "0123456789abcdef0123456789abcdef01234567",
);
const OTHER_COMMIT = RELEASE_COMMIT_SCHEMA.parse(
  "89abcdef0123456789abcdef0123456789abcdef",
);

describe("resolveReleaseMetadata", () => {
  describe("when GitHub dispatches the workflow from a valid tag", () => {
    it("derives the immutable commit, tag, and version from the event", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubRefName: RELEASE_TAG,
          githubRefType: "tag",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        {
          kind: "ok",
          releaseCommit: RELEASE_COMMIT,
          releaseTag: RELEASE_TAG,
          releaseVersion: PACKAGE_VERSION,
        },
      );
    });
  });

  describe("when GitHub dispatches the workflow from a branch", () => {
    it("rejects the mutable source", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubRefName: "main",
          githubRefType: "branch",
          tagCommit: null,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        {
          kind: "release_ref_not_tag",
          actualRefName: "main",
          actualRefType: "branch",
        },
      );
    });
  });

  describe("when the selected tag is malformed", () => {
    it("rejects it before inspecting package versions", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubRefName: "release",
          githubRefType: "tag",
          tagCommit: null,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        { kind: "invalid_release_tag", releaseTag: "release" },
      );
    });
  });

  describe("when the selected tag no longer exists", () => {
    it("rejects the release", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubRefName: RELEASE_TAG,
          githubRefType: "tag",
          tagCommit: null,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        { kind: "release_tag_not_found", releaseTag: RELEASE_TAG },
      );
    });
  });

  describe("when GitHub supplies an invalid event commit", () => {
    it("rejects the release", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: "main",
          githubRefName: RELEASE_TAG,
          githubRefType: "tag",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        { kind: "invalid_release_event_commit", eventCommit: "main" },
      );
    });
  });

  describe("when the selected tag resolves to a different commit", () => {
    it("rejects the moved tag", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: OTHER_COMMIT,
          githubRefName: RELEASE_TAG,
          githubRefType: "tag",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        {
          kind: "release_commit_mismatch",
          expectedCommit: OTHER_COMMIT,
          tagCommit: RELEASE_COMMIT,
        },
      );
    });
  });

  describe("when checkout does not match the selected tag", () => {
    it("rejects the release before building", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubRefName: RELEASE_TAG,
          githubRefType: "tag",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: OTHER_COMMIT,
        }),
        {
          kind: "release_checkout_mismatch",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: OTHER_COMMIT,
        },
      );
    });
  });

  describe("when Cargo metadata is malformed", () => {
    it("returns a schema failure", () => {
      assert.equal(
        resolveReleaseMetadata({
          cargoMetadata: { packages: "not-an-array" },
          githubEventCommit: RELEASE_COMMIT,
          githubRefName: RELEASE_TAG,
          githubRefType: "tag",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }).kind,
        "invalid_cargo_metadata",
      );
    });
  });
});
