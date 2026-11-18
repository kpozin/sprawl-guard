import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { RUNTIME_PACKAGE_DEFINITIONS } from "../package-manifests.ts";
import { WHEEL_PLATFORMS, wheelPlatformTag } from "./wheel-platforms.ts";

/** The wheel tag spelling of each Node.js architecture that a native target uses. */
const WHEEL_ARCHITECTURES = { arm64: "aarch64", x64: "x86_64" } as const;
/** The Linux wheel policy family for each C library. */
const WHEEL_FAMILIES = { gnu: "manylinux", musl: "musllinux" } as const;
/** How the macOS and Windows targets' wheel tags begin and end. */
const OTHER_TAG_PARTS = {
  "darwin-arm64": ["macosx_", "_arm64"],
  "darwin-x64": ["macosx_", "_x86_64"],
  "win32-x64-msvc": ["win_", "_amd64"],
} as const;

describe("WHEEL_PLATFORMS", () => {
  for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
    if (definition.kind !== "native") {
      continue;
    }
    describe(`for ${definition.slug}`, () => {
      const platform = WHEEL_PLATFORMS[definition.slug];

      if (definition.platform === "linux") {
        it("declares the target's C library and architecture", () => {
          assert.ok(platform.kind === "linux");
          assert.equal(platform.policy.family, WHEEL_FAMILIES[definition.libcFamily]);
          assert.equal(platform.policy.arch, WHEEL_ARCHITECTURES[definition.arch]);
        });
      } else {
        it("declares the target's operating system and architecture", () => {
          const [prefix, suffix] = OTHER_TAG_PARTS[definition.slug];
          const tag = wheelPlatformTag(platform);

          assert.ok(tag.startsWith(prefix));
          assert.ok(tag.endsWith(suffix));
        });
      }
    });
  }
});

describe("wheelPlatformTag", () => {
  describe("with a Linux platform", () => {
    it("spells the policy", () => {
      assert.equal(wheelPlatformTag(WHEEL_PLATFORMS["linux-x64-musl"]), "musllinux_1_2_x86_64");
    });
  });

  describe("with another platform", () => {
    it("uses the fixed tag", () => {
      assert.equal(wheelPlatformTag(WHEEL_PLATFORMS["win32-x64-msvc"]), "win_amd64");
    });
  });
});
