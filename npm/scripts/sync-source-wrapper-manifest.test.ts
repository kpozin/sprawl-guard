import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { syncSourceWrapperManifest } from "./sync-source-wrapper-manifest.ts";

describe("syncSourceWrapperManifest", () => {
  let tempWorkspaceRoot: string;

  beforeEach(() => {
    tempWorkspaceRoot = fs.mkdtempSync(
      path.join(os.tmpdir(), "sprawl-guard-public-source-wrapper-manifest-"),
    );
  });

  afterEach(() => {
    fs.rmSync(tempWorkspaceRoot, { recursive: true, force: true });
  });

  describe("when the checked-in source wrapper manifest is missing", () => {
    it("writes the generated manifest", () => {
      const result = syncSourceWrapperManifest({
        mode: "write",
        workspaceRootDirectory: tempWorkspaceRoot,
      });

      assert.equal(result.kind, "ok");

      const wrapperManifest = JSON.parse(
        fs.readFileSync(
          path.join(tempWorkspaceRoot, "sprawl-guard/package.json"),
          "utf8",
        ),
      );

      assert.equal(wrapperManifest.name, "@kpozin/sprawl-guard");
      assert.equal(wrapperManifest.private, true);
      assert.equal(
        wrapperManifest.scripts.build,
        "pnpm run clean && pnpm --dir .. exec tsc -p sprawl-guard/tsconfig.json",
      );
      assert.ok(!("optionalDependencies" in wrapperManifest));
      assert.ok(!("publishConfig" in wrapperManifest));
    });
  });

  describe("when the checked-in source wrapper manifest drifts", () => {
    it("reports the manifest path in check mode", () => {
      syncSourceWrapperManifest({
        mode: "write",
        workspaceRootDirectory: tempWorkspaceRoot,
      });

      fs.writeFileSync(
        path.join(tempWorkspaceRoot, "sprawl-guard/package.json"),
        "{}\n",
      );

      const result = syncSourceWrapperManifest({
        mode: "check",
        workspaceRootDirectory: tempWorkspaceRoot,
      });

      assert.deepEqual(result, {
        kind: "manifest_out_of_sync",
        relativePath: "sprawl-guard/package.json",
      });
    });
  });
});
