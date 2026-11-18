import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { checkLinuxTag, describeLinuxTagGateFailure } from "./linux-tag-gate.ts";
import type { LinuxWheelPolicy } from "./wheel-platforms.ts";

const DECLARED: LinuxWheelPolicy = { family: "manylinux", major: 2, minor: 35, arch: "x86_64" };

/** Renders the parts of an `auditwheel show --json` report that the gate reads. */
function report(overallTag: string, overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    version: 1,
    wheel: "sprawl_guard-0.5.2-py3-none-manylinux_2_35_x86_64.whl",
    pure: false,
    overall_tag: overallTag,
    external_libs: {},
    ...overrides,
  });
}

describe("checkLinuxTag", () => {
  describe("when the binary needs the declared policy or an older one", () => {
    for (const overallTag of ["manylinux_2_35_x86_64", "manylinux_2_34_x86_64", "manylinux_1_99_x86_64"]) {
      it(`accepts ${overallTag}`, () => {
        assert.equal(checkLinuxTag(DECLARED, report(overallTag)).kind, "ok");
      });
    }
  });

  describe("when the binary needs a newer system than the tag declares", () => {
    for (const overallTag of ["manylinux_2_36_x86_64", "manylinux_3_0_x86_64"]) {
      it(`rejects ${overallTag}`, () => {
        const result = checkLinuxTag(DECLARED, report(overallTag));

        assert.equal(result.kind, "declared_too_old");
      });
    }

    it("names both policies", () => {
      const result = checkLinuxTag(DECLARED, report("manylinux_2_39_x86_64"));

      assert.ok(result.kind !== "ok");
      assert.equal(
        describeLinuxTagGateFailure(result),
        "the wheel declares manylinux_2_35_x86_64, but its binary needs manylinux_2_39_x86_64",
      );
    });
  });

  describe("when the binary is for another libc or architecture", () => {
    for (const overallTag of ["musllinux_1_2_x86_64", "manylinux_2_17_aarch64"]) {
      it(`rejects ${overallTag}`, () => {
        assert.equal(checkLinuxTag(DECLARED, report(overallTag)).kind, "policy_mismatch");
      });
    }
  });

  describe("when auditwheel infers no manylinux or musllinux policy", () => {
    for (const overallTag of ["linux_x86_64", "manylinux2014_x86_64", "manylinux_2_35_i686", ""]) {
      it(`rejects ${JSON.stringify(overallTag)}`, () => {
        assert.deepEqual(checkLinuxTag(DECLARED, report(overallTag)), {
          kind: "unrecognized_policy",
          overallTag,
        });
      });
    }
  });

  describe("when the wheel holds no native binary", () => {
    it("rejects it", () => {
      assert.deepEqual(checkLinuxTag(DECLARED, report("manylinux_2_35_x86_64", { pure: true })), {
        kind: "pure_wheel",
      });
    });
  });

  describe("when the report is not one the gate understands", () => {
    for (const [name, json] of [
      ["invalid JSON", "{"],
      ["another report version", report("manylinux_2_35_x86_64", { version: 2 })],
      ["a missing tag", JSON.stringify({ version: 1, pure: false })],
    ] as const) {
      it(`rejects ${name}`, () => {
        assert.equal(checkLinuxTag(DECLARED, json).kind, "invalid_report");
      });
    }
  });
});
