import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  linuxLibcFromReportedGlibcVersion,
  resolveNativePackageName,
  type LinuxLibc,
  type NativeRuntime,
} from "./resolve-package.js";
import { GLIBC_FLOOR } from "./runtime-packages.js";

function runtime(
  platform: Exclude<NodeJS.Platform, "linux">,
  arch: NodeJS.Architecture,
): NativeRuntime {
  return {
    platform,
    arch,
  };
}

function linuxRuntime(arch: NodeJS.Architecture, libc: LinuxLibc): NativeRuntime {
  return {
    platform: "linux",
    arch,
    libc,
  };
}

const MUSL: LinuxLibc = { family: "musl" };
const GLIBC_AT_FLOOR: LinuxLibc = { family: "gnu", version: GLIBC_FLOOR };
const GLIBC_BELOW_FLOOR: LinuxLibc = {
  family: "gnu",
  version: { major: GLIBC_FLOOR.major, minor: GLIBC_FLOOR.minor - 1 },
};

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
        resolveNativePackageName(linuxRuntime("arm64", GLIBC_AT_FLOOR)),
        "@kpozin/sprawl-guard-linux-arm64-gnu",
      );
      assert.equal(
        resolveNativePackageName(linuxRuntime("x64", GLIBC_AT_FLOOR)),
        "@kpozin/sprawl-guard-linux-x64-gnu",
      );
      assert.equal(
        resolveNativePackageName(linuxRuntime("x64", MUSL)),
        "@kpozin/sprawl-guard-linux-x64-musl",
      );
      assert.equal(
        resolveNativePackageName(runtime("win32", "x64")),
        "@kpozin/sprawl-guard-win32-x64-msvc",
      );
    });
  });

  describe("when glibc is newer than the floor in its major or minor version", () => {
    test("returns the glibc package", () => {
      for (const version of [
        { major: GLIBC_FLOOR.major, minor: GLIBC_FLOOR.minor + 1 },
        { major: GLIBC_FLOOR.major + 1, minor: 0 },
      ]) {
        assert.equal(
          resolveNativePackageName(linuxRuntime("x64", { family: "gnu", version })),
          "@kpozin/sprawl-guard-linux-x64-gnu",
        );
      }
    });
  });

  describe("when glibc is older than the native binaries need", () => {
    test("returns null so the wrapper can fall back to WASI", () => {
      assert.equal(resolveNativePackageName(linuxRuntime("x64", GLIBC_BELOW_FLOOR)), null);
      assert.equal(resolveNativePackageName(linuxRuntime("arm64", GLIBC_BELOW_FLOOR)), null);
    });
  });

  describe("when the runtime has no native package", () => {
    test("returns null so the wrapper can fall back to WASI", () => {
      assert.equal(resolveNativePackageName(linuxRuntime("arm64", MUSL)), null);
      assert.equal(resolveNativePackageName(runtime("freebsd", "x64")), null);
    });
  });
});

describe("linuxLibcFromReportedGlibcVersion", () => {
  describe("when Node reports a glibc version", () => {
    test("returns glibc with the major and minor version", () => {
      assert.deepEqual(linuxLibcFromReportedGlibcVersion("2.34"), {
        family: "gnu",
        version: { major: 2, minor: 34 },
      });
      assert.deepEqual(linuxLibcFromReportedGlibcVersion("2.35.9000"), {
        family: "gnu",
        version: { major: 2, minor: 35 },
      });
    });
  });

  describe("when Node reports no glibc version", () => {
    test("returns musl", () => {
      assert.deepEqual(linuxLibcFromReportedGlibcVersion(undefined), MUSL);
    });
  });

  describe("when the reported version is not a version number", () => {
    test("returns null, so no native package is chosen", () => {
      assert.equal(linuxLibcFromReportedGlibcVersion("unknown"), null);
    });
  });
});
