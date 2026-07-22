import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { resolveReleaseMetadata } from "./release-metadata.ts";

const MATCHING_CARGO_METADATA = {
  packages: [
    { name: "sprawl-guard", version: "0.1.0" },
    { name: "sprawl-guard-lib", version: "0.1.0" },
  ],
};
const RELEASE_COMMIT = "0123456789abcdef0123456789abcdef01234567";
const OTHER_COMMIT = "89abcdef0123456789abcdef0123456789abcdef";

function resolveManualReleaseMetadata(
  args: Omit<Parameters<typeof resolveReleaseMetadata>[0], "githubEventName">,
) {
  return resolveReleaseMetadata({
    ...args,
    githubEventName: "workflow_dispatch",
  });
}

describe("resolveReleaseMetadata", () => {
  describe("when a manual run supplies a valid existing tag", () => {
    it("returns the immutable commit, tag, and version", () => {
      assert.deepEqual(
        resolveManualReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: OTHER_COMMIT,
          githubRefName: "main",
          githubRefType: "branch",
          inputReleaseCommit: RELEASE_COMMIT,
          inputReleaseTag: "v0.1.0",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        {
          kind: "ok",
          releaseCommit: RELEASE_COMMIT,
          releaseTag: "v0.1.0",
          releaseVersion: "0.1.0",
        },
      );
    });
  });

  describe("when a manual run is launched from a tag ref", () => {
    it("validates the recorded commit instead of trusting the event ref", () => {
      assert.deepEqual(
        resolveManualReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubRefName: "v0.1.0",
          githubRefType: "tag",
          inputReleaseCommit: OTHER_COMMIT,
          inputReleaseTag: "v0.1.0",
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

  describe("when a tag push omits the manual input", () => {
    it("uses the pushed tag", () => {
      assert.equal(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubEventName: "push",
          githubRefName: "v0.1.0",
          githubRefType: "tag",
          inputReleaseCommit: "",
          inputReleaseTag: "",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }).kind,
        "ok",
      );
    });
  });

  describe("when a published-release event supplies the tag", () => {
    it("uses the event commit selected by GitHub Pages", () => {
      assert.equal(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubEventName: "release",
          githubRefName: "v0.1.0",
          githubRefType: "tag",
          inputReleaseCommit: "",
          inputReleaseTag: "v0.1.0",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }).kind,
        "ok",
      );
    });
  });

  describe("when a pushed tag moved before checkout", () => {
    it("rejects the fetched tag instead of replacing the event commit", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: OTHER_COMMIT,
          githubEventName: "push",
          githubRefName: "v0.1.0",
          githubRefType: "tag",
          inputReleaseCommit: "",
          inputReleaseTag: "",
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

  describe("when a manual run omits the tag", () => {
    it("rejects the request", () => {
      assert.deepEqual(
        resolveManualReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: OTHER_COMMIT,
          githubRefName: "main",
          githubRefType: "branch",
          inputReleaseCommit: RELEASE_COMMIT,
          inputReleaseTag: "",
          tagCommit: null,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        { kind: "missing_release_tag" },
      );
    });
  });

  describe("when a manual run omits the expected release commit", () => {
    it("rejects the request", () => {
      assert.deepEqual(
        resolveManualReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: OTHER_COMMIT,
          githubRefName: "main",
          githubRefType: "branch",
          inputReleaseCommit: "",
          inputReleaseTag: "v0.1.0",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        { kind: "missing_release_commit" },
      );
    });
  });

  describe("when a manual run supplies a non-SHA release commit", () => {
    it("rejects the request", () => {
      assert.deepEqual(
        resolveManualReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: OTHER_COMMIT,
          githubRefName: "main",
          githubRefType: "branch",
          inputReleaseCommit: "main",
          inputReleaseTag: "v0.1.0",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        { kind: "invalid_release_commit", releaseCommit: "main" },
      );
    });
  });

  describe("when the tag does not match the manually recorded release commit", () => {
    it("rejects the request", () => {
      assert.deepEqual(
        resolveManualReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: OTHER_COMMIT,
          githubRefName: "main",
          githubRefType: "branch",
          inputReleaseCommit: OTHER_COMMIT,
          inputReleaseTag: "v0.1.0",
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

  describe("when checkout does not match the release tag", () => {
    it("rejects the request before building", () => {
      assert.deepEqual(
        resolveManualReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: OTHER_COMMIT,
          githubRefName: "main",
          githubRefType: "branch",
          inputReleaseCommit: RELEASE_COMMIT,
          inputReleaseTag: "v0.1.0",
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
        resolveManualReleaseMetadata({
          cargoMetadata: { packages: "not-an-array" },
          githubEventCommit: OTHER_COMMIT,
          githubRefName: "main",
          githubRefType: "branch",
          inputReleaseCommit: RELEASE_COMMIT,
          inputReleaseTag: "v0.1.0",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }).kind,
        "invalid_cargo_metadata",
      );
    });
  });

  describe("when the workflow event is not a release entry point", () => {
    it("rejects the event before selecting a commit source", () => {
      assert.deepEqual(
        resolveReleaseMetadata({
          cargoMetadata: MATCHING_CARGO_METADATA,
          githubEventCommit: RELEASE_COMMIT,
          githubEventName: "pull_request",
          githubRefName: "main",
          githubRefType: "branch",
          inputReleaseCommit: RELEASE_COMMIT,
          inputReleaseTag: "v0.1.0",
          tagCommit: RELEASE_COMMIT,
          workingTreeCommit: RELEASE_COMMIT,
        }),
        { kind: "invalid_release_event", eventName: "pull_request" },
      );
    });
  });
});
