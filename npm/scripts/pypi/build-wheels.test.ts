import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { RUNTIME_PACKAGE_DEFINITIONS } from "../package-manifests.ts";
import { absoluteDirectoryPath, artifactRootDirectory } from "../path-values.ts";
import {
  buildWheels,
  describeBuildWheelsFailure,
  type BuildWheelsArgs,
  type RunTool,
  type ToolOutput,
} from "./build-wheels.ts";
import type { PythonPackageVersion } from "./python-version.ts";
import type { WheelPlatformTag } from "./wheel-platforms.ts";
import { WHEEL_LICENSE_FILE_NAMES, wheelFileName } from "./wheel-tree.ts";

const VERSION = "0.5.2" as PythonPackageVersion;

/** The policy that the release binaries really need, keyed by the tag a wheel declares. */
const INFERRED_TAGS: Readonly<Record<string, string>> = {
  manylinux_2_35_aarch64: "manylinux_2_34_aarch64",
  manylinux_2_35_x86_64: "manylinux_2_34_x86_64",
  musllinux_1_2_x86_64: "musllinux_1_2_x86_64",
  // The gate's self-test packs the linux-x64-gnu binary under this older tag.
  manylinux_2_17_x86_64: "manylinux_2_34_x86_64",
};

/** Behavior of the fake Python tools, which by default act like the real ones on good input. */
interface FakeTools {
  readonly metadata?: ToolOutput;
  readonly pack?: "ok" | "fail" | "write_nothing";
  readonly inferredTags?: Readonly<Record<string, string>>;
  readonly auditwheelFailsFor?: string;
}

/** Records each tool call and imitates `python`, `wheel pack`, and `auditwheel show --json`. */
function fakeRunTool(tools: FakeTools, calls: string[][]): RunTool {
  const inferredTags = tools.inferredTags ?? INFERRED_TAGS;
  return (command, args) => {
    calls.push([command, ...args]);
    if (command === "python" && args.includes("-c")) {
      return tools.metadata ?? { kind: "ok", stdout: "" };
    }
    if (command === "python") {
      const [tree, , destination] = args.slice(-3);
      assert.ok(tree !== undefined && destination !== undefined);
      if (tools.pack === "fail") {
        return { kind: "failed", detail: "pack exploded" };
      }
      if (tools.pack !== "write_nothing") {
        // The fake reads back the tag that buildWheels named the tree after.
        const platformTag = path.basename(path.dirname(tree)) as WheelPlatformTag;
        fs.writeFileSync(path.join(destination, wheelFileName(VERSION, platformTag)), "");
      }
      return { kind: "ok", stdout: "" };
    }
    const wheelPath = args.at(-1) ?? "";
    const platformTag = /-py3-none-(?<tag>.+)\.whl$/u.exec(wheelPath)?.groups?.tag ?? "";
    if (platformTag === tools.auditwheelFailsFor) {
      return { kind: "failed", detail: "auditwheel exited with 1" };
    }
    return {
      kind: "ok",
      stdout: JSON.stringify({ version: 1, pure: false, overall_tag: inferredTags[platformTag] }),
    };
  };
}

describe("buildWheels", () => {
  let directory = "";
  let calls: string[][] = [];

  beforeEach(() => {
    directory = fs.mkdtempSync(path.join(os.tmpdir(), "build-wheels-test-"));
    calls = [];
    for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
      const slugDirectory = path.join(directory, "artifacts", definition.slug);
      fs.mkdirSync(slugDirectory, { recursive: true });
      fs.writeFileSync(path.join(slugDirectory, definition.artifactRelativePath), definition.slug);
    }
    for (const name of WHEEL_LICENSE_FILE_NAMES) {
      fs.writeFileSync(path.join(directory, name), name);
    }
  });

  afterEach(() => {
    fs.rmSync(directory, { recursive: true, force: true });
  });

  function run(tools: FakeTools = {}) {
    const args: BuildWheelsArgs = {
      artifactDirectory: artifactRootDirectory(path.join(directory, "artifacts")),
      outputDirectory: absoluteDirectoryPath(path.join(directory, "out")),
      workDirectory: absoluteDirectoryPath(path.join(directory, "work")),
      version: VERSION,
      readme: "# sprawl-guard\n",
      licenseDirectory: absoluteDirectoryPath(directory),
      runTool: fakeRunTool(tools, calls),
    };
    return buildWheels(args);
  }

  describe("when every tool succeeds and every Linux tag passes", () => {
    it("builds one wheel per native target", () => {
      const result = run();

      assert.deepEqual(result, {
        kind: "ok",
        fileNames: [
          "sprawl_guard-0.5.2-py3-none-macosx_11_0_arm64.whl",
          "sprawl_guard-0.5.2-py3-none-macosx_10_12_x86_64.whl",
          "sprawl_guard-0.5.2-py3-none-manylinux_2_35_aarch64.whl",
          "sprawl_guard-0.5.2-py3-none-manylinux_2_35_x86_64.whl",
          "sprawl_guard-0.5.2-py3-none-musllinux_1_2_x86_64.whl",
          "sprawl_guard-0.5.2-py3-none-win_amd64.whl",
        ],
      });
      assert.deepEqual(fs.readdirSync(path.join(directory, "out")).sort(), [...result.fileNames].sort());
    });

    it("audits the Linux wheels and the self-test wheel only", () => {
      run();

      const audited = calls.filter(([command]) => command === "auditwheel").map((call) => path.basename(call.at(-1) ?? ""));
      assert.deepEqual(audited, [
        "sprawl_guard-0.5.2-py3-none-manylinux_2_35_aarch64.whl",
        "sprawl_guard-0.5.2-py3-none-manylinux_2_35_x86_64.whl",
        "sprawl_guard-0.5.2-py3-none-musllinux_1_2_x86_64.whl",
        "sprawl_guard-0.5.2-py3-none-manylinux_2_17_x86_64.whl",
      ]);
    });

    it("keeps the self-test wheel out of the output", () => {
      run();

      assert.ok(!fs.readdirSync(path.join(directory, "out")).some((name) => name.includes("2_17")));
    });
  });

  describe("when the metadata is invalid", () => {
    it("fails before packing anything", () => {
      const result = run({ metadata: { kind: "failed", detail: "bad Name" } });

      assert.deepEqual(result, { kind: "invalid_metadata", detail: "bad Name" });
      assert.equal(calls.length, 1);
    });
  });

  describe("when wheel pack fails", () => {
    it("names the wheel", () => {
      const result = run({ pack: "fail" });

      assert.deepEqual(result, {
        kind: "pack_failed",
        fileName: "sprawl_guard-0.5.2-py3-none-macosx_11_0_arm64.whl",
        detail: "pack exploded",
      });
    });
  });

  describe("when wheel pack writes no wheel", () => {
    it("names the missing wheel", () => {
      assert.deepEqual(run({ pack: "write_nothing" }), {
        kind: "missing_wheel",
        fileName: "sprawl_guard-0.5.2-py3-none-macosx_11_0_arm64.whl",
      });
    });
  });

  describe("when auditwheel fails", () => {
    it("names the wheel", () => {
      const result = run({ auditwheelFailsFor: "manylinux_2_35_x86_64" });

      assert.deepEqual(result, {
        kind: "auditwheel_failed",
        fileName: "sprawl_guard-0.5.2-py3-none-manylinux_2_35_x86_64.whl",
        detail: "auditwheel exited with 1",
      });
    });
  });

  describe("when a Linux binary needs a newer system than its tag declares", () => {
    it("rejects the wheel", () => {
      const result = run({
        inferredTags: { ...INFERRED_TAGS, manylinux_2_35_aarch64: "manylinux_2_39_aarch64" },
      });

      assert.equal(result.kind, "tag_rejected");
      assert.ok(result.kind === "tag_rejected");
      assert.equal(
        describeBuildWheelsFailure(result),
        "sprawl_guard-0.5.2-py3-none-manylinux_2_35_aarch64.whl: the wheel declares manylinux_2_35_aarch64, but its binary needs manylinux_2_39_aarch64",
      );
    });
  });

  describe("when the gate accepts the too-old self-test tag", () => {
    it("fails the build", () => {
      const result = run({
        inferredTags: { ...INFERRED_TAGS, manylinux_2_17_x86_64: "manylinux_2_17_x86_64" },
      });

      assert.equal(result.kind, "gate_self_test_failed");
      assert.ok(result.kind === "gate_self_test_failed");
      assert.equal(
        describeBuildWheelsFailure(result),
        "the tag gate accepted a wheel tagged manylinux_2_17_x86_64, which is older than its binary supports (ok)",
      );
    });
  });
});
