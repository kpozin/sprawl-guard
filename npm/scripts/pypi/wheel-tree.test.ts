import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { absoluteDirectoryPath } from "../path-values.ts";
import type { PythonPackageVersion } from "./python-version.ts";
import { WHEEL_PLATFORMS, wheelPlatformTag } from "./wheel-platforms.ts";
import {
  renderMetadata,
  renderWheelFile,
  WHEEL_LICENSE_FILE_NAMES,
  wheelFileName,
  writeWheelTree,
} from "./wheel-tree.ts";

const VERSION = "0.5.2b1" as PythonPackageVersion;
const WINDOWS_TAG = wheelPlatformTag(WHEEL_PLATFORMS["win32-x64-msvc"]);
const MACOS_TAG = wheelPlatformTag(WHEEL_PLATFORMS["darwin-arm64"]);
const LINUX_TAG = wheelPlatformTag(WHEEL_PLATFORMS["linux-x64-gnu"]);

describe("wheelFileName", () => {
  it("joins the distribution, version, and tags", () => {
    assert.equal(
      wheelFileName(VERSION, WINDOWS_TAG),
      "sprawl_guard-0.5.2b1-py3-none-win_amd64.whl",
    );
  });
});

describe("renderMetadata", () => {
  const metadata = renderMetadata(VERSION, "# sprawl-guard\n");

  it("names the distribution and version", () => {
    assert.match(metadata, /^Metadata-Version: 2\.4\nName: sprawl-guard\nVersion: 0\.5\.2b1\n/u);
  });

  it("lists every license file", () => {
    for (const name of WHEEL_LICENSE_FILE_NAMES) {
      assert.ok(metadata.includes(`License-File: ${name}\n`));
    }
  });

  it("puts the README after a blank line", () => {
    assert.ok(metadata.endsWith("Description-Content-Type: text/markdown\n\n# sprawl-guard\n"));
  });
});

describe("renderWheelFile", () => {
  it("marks the wheel as platform-specific", () => {
    assert.equal(
      renderWheelFile(MACOS_TAG),
      "Wheel-Version: 1.0\nGenerator: sprawl-guard-release\nRoot-Is-Purelib: false\nTag: py3-none-macosx_11_0_arm64\n",
    );
  });
});

describe("writeWheelTree", () => {
  let directory = "";

  beforeEach(() => {
    directory = fs.mkdtempSync(path.join(os.tmpdir(), "wheel-tree-test-"));
    fs.writeFileSync(path.join(directory, "binary"), "binary");
    fs.chmodSync(path.join(directory, "binary"), 0o644);
    for (const name of WHEEL_LICENSE_FILE_NAMES) {
      fs.writeFileSync(path.join(directory, name), name);
    }
  });

  afterEach(() => {
    fs.rmSync(directory, { recursive: true, force: true });
  });

  describe("when given a binary and license files", () => {
    it("lays out the wheel's contents", () => {
      const root = writeWheelTree(path.join(directory, "tree"), {
        version: VERSION,
        platformTag: WINDOWS_TAG,
        binaryPath: path.join(directory, "binary"),
        binaryFileName: "sprawl-guard.exe",
        readme: "readme",
        licenseDirectory: absoluteDirectoryPath(directory),
      });

      assert.equal(root, path.join(directory, "tree", "sprawl_guard-0.5.2b1"));
      const files = fs.readdirSync(root, { recursive: true, encoding: "utf8" }).sort();
      assert.deepEqual(files, [
        "sprawl_guard-0.5.2b1.data",
        path.join("sprawl_guard-0.5.2b1.data", "scripts"),
        path.join("sprawl_guard-0.5.2b1.data", "scripts", "sprawl-guard.exe"),
        "sprawl_guard-0.5.2b1.dist-info",
        path.join("sprawl_guard-0.5.2b1.dist-info", "METADATA"),
        path.join("sprawl_guard-0.5.2b1.dist-info", "WHEEL"),
        path.join("sprawl_guard-0.5.2b1.dist-info", "licenses"),
        path.join("sprawl_guard-0.5.2b1.dist-info", "licenses", "LICENSE-APACHE"),
        path.join("sprawl_guard-0.5.2b1.dist-info", "licenses", "LICENSE-MIT"),
      ]);
    });

    it("makes the binary executable", () => {
      const root = writeWheelTree(path.join(directory, "tree"), {
        version: VERSION,
        platformTag: LINUX_TAG,
        binaryPath: path.join(directory, "binary"),
        binaryFileName: "sprawl-guard",
        readme: "readme",
        licenseDirectory: absoluteDirectoryPath(directory),
      });

      const binary = path.join(root, "sprawl_guard-0.5.2b1.data", "scripts", "sprawl-guard");
      assert.equal(fs.statSync(binary).mode & 0o777, 0o755);
    });
  });
});
