import type { z } from "zod";
import type { NativeRuntimePackageDefinition } from "../../sprawl-guard/src/lib/runtime-packages.ts";

/** Slug of a native runtime target, such as `linux-x64-gnu`. Each one ships as one wheel. */
export type NativeRuntimePackageSlug = NativeRuntimePackageDefinition["slug"];

/** A wheel platform tag, such as `manylinux_2_35_x86_64`, the last part of a wheel's file name. */
export type WheelPlatformTag = string & z.$brand<"WheelPlatformTag">;

/** The CPU architecture names that Linux wheel tags use. */
export type LinuxWheelArchitecture = "aarch64" | "x86_64";

/**
 * The oldest Linux systems a wheel runs on: glibc or musl at a minimum version, on one
 * architecture. Wheel tags spell it as, for example, `manylinux_2_35_x86_64`.
 */
export interface LinuxWheelPolicy {
  readonly family: "manylinux" | "musllinux";
  readonly major: number;
  readonly minor: number;
  readonly arch: LinuxWheelArchitecture;
}

/**
 * The platform a wheel installs on. Linux tags are structured so that the release can compare
 * them with the policy auditwheel infers from the binary; the others are fixed strings.
 */
export type WheelPlatform =
  | { readonly kind: "linux"; readonly policy: LinuxWheelPolicy }
  | { readonly kind: "macos"; readonly tag: `macosx_${string}` }
  | { readonly kind: "windows"; readonly tag: `win_${string}` };

/**
 * Wheel platform for every native runtime target. The `satisfies` clause makes a new native
 * target fail to compile until it has a wheel platform here too.
 *
 * - The macOS tags match the default deployment targets of the pinned Rust toolchain, which apply
 *   because the release doesn't set `MACOSX_DEPLOYMENT_TARGET`.
 * - The manylinux tags follow the glibc 2.35 of the Ubuntu 22.04 build containers.
 */
export const WHEEL_PLATFORMS = {
  "darwin-arm64": { kind: "macos", tag: "macosx_11_0_arm64" },
  "darwin-x64": { kind: "macos", tag: "macosx_10_12_x86_64" },
  "linux-arm64-gnu": {
    kind: "linux",
    policy: { family: "manylinux", major: 2, minor: 35, arch: "aarch64" },
  },
  "linux-x64-gnu": {
    kind: "linux",
    policy: { family: "manylinux", major: 2, minor: 35, arch: "x86_64" },
  },
  "linux-x64-musl": {
    kind: "linux",
    policy: { family: "musllinux", major: 1, minor: 2, arch: "x86_64" },
  },
  "win32-x64-msvc": { kind: "windows", tag: "win_amd64" },
} as const satisfies Record<NativeRuntimePackageSlug, WheelPlatform>;

/** Spells a Linux policy as a wheel platform tag, such as `manylinux_2_35_x86_64`. */
export function linuxPolicyTag(policy: LinuxWheelPolicy): WheelPlatformTag {
  return `${policy.family}_${policy.major}_${policy.minor}_${policy.arch}` as WheelPlatformTag;
}

/** Spells a wheel platform as the tag that ends the wheel's file name. */
export function wheelPlatformTag(platform: WheelPlatform): WheelPlatformTag {
  return platform.kind === "linux"
    ? linuxPolicyTag(platform.policy)
    : (platform.tag as WheelPlatformTag);
}
