import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  inspectReleaseSource,
  parseReleaseSourceExpectation,
  validateReleaseSource,
} from "./release-source.ts";

const RELEASE_COMMIT = "0123456789abcdef0123456789abcdef01234567";
const OTHER_COMMIT = "89abcdef0123456789abcdef0123456789abcdef";

function releaseExpectation() {
  const parsed = parseReleaseSourceExpectation(
    [RELEASE_COMMIT, "v0.1.0"],
    "0.1.0",
  );
  assert.equal(parsed.kind, "ok");
  if (parsed.kind !== "ok") {
    throw new Error("test release expectation must parse");
  }
  return parsed.expectation;
}

describe("parseReleaseSourceExpectation", () => {
  describe("when a full commit and matching release tag are supplied", () => {
    it("brands the immutable source expectation", () => {
      assert.deepEqual(
        parseReleaseSourceExpectation([RELEASE_COMMIT, "v0.1.0"], "0.1.0"),
        {
          kind: "ok",
          expectation: {
            commit: RELEASE_COMMIT,
            tag: "v0.1.0",
          },
        },
      );
    });
  });

  describe("when the commit or tag is not an immutable release identifier", () => {
    it("rejects the arguments", () => {
      assert.equal(
        parseReleaseSourceExpectation(["main", "0.1.0"], "0.1.0").kind,
        "invalid",
      );
      assert.equal(
        parseReleaseSourceExpectation([RELEASE_COMMIT, "v0.2.0"], "0.1.0").kind,
        "invalid",
      );
    });
  });
});

describe("validateReleaseSource", () => {
  describe("when HEAD, the tag, and the clean tree match the expectation", () => {
    it("accepts the checkout", () => {
      assert.deepEqual(
        validateReleaseSource({
          expectation: releaseExpectation(),
          headCommit: RELEASE_COMMIT,
          status: "",
          tagCommit: RELEASE_COMMIT,
        }),
        { kind: "ok" },
      );
    });
  });

  describe("when HEAD is a different clean release checkout", () => {
    it("rejects the checkout", () => {
      assert.deepEqual(
        validateReleaseSource({
          expectation: releaseExpectation(),
          headCommit: OTHER_COMMIT,
          status: "",
          tagCommit: RELEASE_COMMIT,
        }),
        {
          kind: "head_mismatch",
          actualCommit: OTHER_COMMIT,
          expectedCommit: RELEASE_COMMIT,
        },
      );
    });
  });

  describe("when the release tag resolves to another commit", () => {
    it("rejects the checkout", () => {
      assert.equal(
        validateReleaseSource({
          expectation: releaseExpectation(),
          headCommit: RELEASE_COMMIT,
          status: "",
          tagCommit: OTHER_COMMIT,
        }).kind,
        "tag_mismatch",
      );
    });
  });

  describe("when tracked source files are dirty", () => {
    it("rejects the checkout", () => {
      assert.deepEqual(
        validateReleaseSource({
          expectation: releaseExpectation(),
          headCommit: RELEASE_COMMIT,
          status: " M npm/scripts/publish-npm-packages.ts",
          tagCommit: RELEASE_COMMIT,
        }),
        {
          kind: "dirty_tree",
          status: " M npm/scripts/publish-npm-packages.ts",
        },
      );
    });
  });
});

describe("inspectReleaseSource", () => {
  describe("when Git cannot inspect the workspace", () => {
    it("returns a typed failure instead of throwing", () => {
      const result = inspectReleaseSource({
        expectation: releaseExpectation(),
        workspaceRoot: "/workspace/that-does-not-exist",
      });

      assert.equal(result.kind, "git_failed");
      assert.match(result.detail, /No such file|ENOENT/u);
    });
  });
});
