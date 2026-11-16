import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import {
  PACKAGE_VERSION,
  PUBLISH_WRAPPER_DOCUMENTS,
} from "./package-manifests.ts";
import { renderPublishWorkspace } from "./render-publish-workspace.ts";

describe("renderPublishWorkspace", () => {
  let tempPublicRoot: string;
  let tempWorkspaceRoot: string;

  beforeEach(() => {
    tempPublicRoot = fs.mkdtempSync(
      path.join(os.tmpdir(), "sprawl-guard-public-rendered-workspace-"),
    );
    tempWorkspaceRoot = path.join(tempPublicRoot, "npm");
    fs.mkdirSync(tempWorkspaceRoot);
    fs.writeFileSync(path.join(tempPublicRoot, "LICENSE-MIT"), "mit\n");
    fs.writeFileSync(path.join(tempPublicRoot, "LICENSE-APACHE"), "apache\n");
    for (const document of PUBLISH_WRAPPER_DOCUMENTS) {
      const sourcePath = path.join(tempWorkspaceRoot, document.sourcePath);
      fs.mkdirSync(path.dirname(sourcePath), { recursive: true });
      fs.writeFileSync(sourcePath, `source ${document.sourcePath}\n`);
    }

    fs.mkdirSync(path.join(tempWorkspaceRoot, "sprawl-guard", "dist", "bin"), {
      recursive: true,
    });
    fs.writeFileSync(
      path.join(tempWorkspaceRoot, "sprawl-guard", "README.md"),
      "# wrapper\n",
    );
    fs.writeFileSync(
      path.join(
        tempWorkspaceRoot,
        "sprawl-guard",
        "dist",
        "bin",
        "sprawl-guard.js",
      ),
      "console.log('wrapper');\n",
    );
  });

  afterEach(() => {
    fs.rmSync(tempPublicRoot, { recursive: true, force: true });
  });

  describe("when the source wrapper build output exists", () => {
    it("renders `.stage/` with generated manifests, generated runtime READMEs, and copied wrapper files", () => {
      const result = renderPublishWorkspace({
        workspaceRootDirectory: tempWorkspaceRoot,
      });

      assert.equal(result.kind, "ok");
      const wrapperManifest = JSON.parse(
        fs.readFileSync(
          path.join(
            tempWorkspaceRoot,
            ".stage",
            "sprawl-guard",
            "package.json",
          ),
          "utf8",
        ),
      );
      assert.equal(
        wrapperManifest.optionalDependencies[
          "@kpozin/sprawl-guard-linux-x64-gnu"
        ],
        PACKAGE_VERSION,
      );
      assert.deepEqual(wrapperManifest.files, [
        "README.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "schema/",
        "skills/",
        "dist/",
      ]);
      for (const document of PUBLISH_WRAPPER_DOCUMENTS) {
        assert.equal(
          fs.readFileSync(
            path.join(
              tempWorkspaceRoot,
              ".stage",
              "sprawl-guard",
              document.packagePath,
            ),
            "utf8",
          ),
          `source ${document.sourcePath}\n`,
        );
      }
      assert.equal(
        fs.readFileSync(
          path.join(tempWorkspaceRoot, ".stage", "sprawl-guard", "LICENSE-MIT"),
          "utf8",
        ),
        fs.readFileSync(
          path.join(tempWorkspaceRoot, "..", "LICENSE-MIT"),
          "utf8",
        ),
      );
      assert.equal(
        fs.readFileSync(
          path.join(
            tempWorkspaceRoot,
            ".stage",
            "packages",
            "linux-x64-gnu",
            "LICENSE-APACHE",
          ),
          "utf8",
        ),
        fs.readFileSync(
          path.join(tempWorkspaceRoot, "..", "LICENSE-APACHE"),
          "utf8",
        ),
      );
      assert.equal(
        fs.readFileSync(
          path.join(
            tempWorkspaceRoot,
            ".stage",
            "sprawl-guard",
            "dist",
            "bin",
            "sprawl-guard.js",
          ),
          "utf8",
        ),
        "console.log('wrapper');\n",
      );
      assert.equal(
        fs.readFileSync(
          path.join(
            tempWorkspaceRoot,
            ".stage",
            "packages",
            "linux-x64-gnu",
            "README.md",
          ),
          "utf8",
        ),
        [
          "# `@kpozin/sprawl-guard-linux-x64-gnu`",
          "",
          "This package carries the linux x64 glibc native `sprawl-guard` binary.",
          "Install `@kpozin/sprawl-guard` instead; it selects this package automatically when needed.",
          "",
          "See the [sprawl-guard project](https://github.com/kpozin/sprawl-guard#readme).",
          "",
        ].join("\n"),
      );
    });
  });

  describe("when the source wrapper build output is missing", () => {
    it("fails before creating a partial staged workspace", () => {
      fs.rmSync(path.join(tempWorkspaceRoot, "sprawl-guard", "dist"), {
        recursive: true,
        force: true,
      });

      const result = renderPublishWorkspace({
        workspaceRootDirectory: tempWorkspaceRoot,
      });

      assert.deepEqual(result, {
        kind: "missing_source_entry",
        relativePath: "sprawl-guard/dist",
      });
      assert.ok(!fs.existsSync(path.join(tempWorkspaceRoot, ".stage")));
    });
  });

  describe("when the workspace root cannot hold a temporary stage directory", () => {
    it("returns a structured filesystem error", () => {
      const result = renderPublishWorkspace({
        workspaceRootDirectory: path.join(tempWorkspaceRoot, "missing-root"),
      });

      assert.equal(result.kind, "filesystem_error");
      if (result.kind === "filesystem_error") {
        assert.equal(result.operation, "create");
        assert.equal(result.relativePath, ".stage.tmp-*");
      }
    });
  });

  describe("when replacing an existing stage fails", () => {
    it("keeps the previous rendered workspace in place", () => {
      fs.mkdirSync(path.join(tempWorkspaceRoot, ".stage"), { recursive: true });
      fs.writeFileSync(
        path.join(tempWorkspaceRoot, ".stage", "sentinel"),
        "old\n",
      );

      const originalRenameSync = fs.renameSync;
      fs.renameSync = ((oldPath: fs.PathLike, newPath: fs.PathLike) => {
        if (
          String(oldPath).includes(".stage.tmp-") &&
          String(newPath).endsWith(".stage")
        ) {
          throw new Error("simulated final rename failure");
        }
        return originalRenameSync(oldPath, newPath);
      }) as typeof fs.renameSync;

      try {
        const result = renderPublishWorkspace({
          workspaceRootDirectory: tempWorkspaceRoot,
        });

        assert.equal(result.kind, "filesystem_error");
        if (result.kind === "filesystem_error") {
          assert.equal(result.operation, "rename");
          assert.equal(result.relativePath, ".stage");
        }
        assert.equal(
          fs.readFileSync(
            path.join(tempWorkspaceRoot, ".stage", "sentinel"),
            "utf8",
          ),
          "old\n",
        );
      } finally {
        fs.renameSync = originalRenameSync;
      }
    });
  });
});
