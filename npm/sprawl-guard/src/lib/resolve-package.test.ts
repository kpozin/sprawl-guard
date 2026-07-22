import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  resolveNativePackageName,
  type LibcFamily,
  type NativeRuntime,
} from "./resolve-package.js";

function runtime(
  platform: Exclude<NodeJS.Platform, "linux">,
  arch: NodeJS.Architecture,
): NativeRuntime {
  return {
    platform,
    arch,
  };
}

function linuxRuntime(
  arch: NodeJS.Architecture,
  libcFamily: LibcFamily,
): NativeRuntime {
  return {
    platform: "linux",
    arch,
    libcFamily,
  };
}

describe("resolveNativePackageName", () => {
  describe("when the runtime is supported by a native package", () => {
    test("returns the exact platform, architecture, and libc package name", () => {
      assert.equal(
        resolveNativePackageName(runtime("darwin", "arm64")),
        "@kpozin/sprawl-guard-darwin-arm64",
      );
      assert.equal(
        resolveNativePackageName(runtime("darwin", "x64")),
        "@kpozin/sprawl-guard-darwin-x64",
      );
      assert.equal(
        resolveNativePackageName(linuxRuntime("arm64", "gnu")),
        "@kpozin/sprawl-guard-linux-arm64-gnu",
      );
      assert.equal(
        resolveNativePackageName(linuxRuntime("x64", "gnu")),
        "@kpozin/sprawl-guard-linux-x64-gnu",
      );
      assert.equal(
        resolveNativePackageName(linuxRuntime("x64", "musl")),
        "@kpozin/sprawl-guard-linux-x64-musl",
      );
      assert.equal(
        resolveNativePackageName(runtime("win32", "x64")),
        "@kpozin/sprawl-guard-win32-x64-msvc",
      );
    });
  });

  describe("when the runtime has no native package", () => {
    test("returns null so the wrapper can fall back to WASI", () => {
      assert.equal(resolveNativePackageName(linuxRuntime("arm64", "musl")), null);
      assert.equal(resolveNativePackageName(runtime("freebsd", "x64")), null);
    });
  });
});
