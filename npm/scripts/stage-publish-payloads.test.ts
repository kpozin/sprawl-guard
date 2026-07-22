import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { RUNTIME_PACKAGE_DEFINITIONS } from "./package-manifests.ts";
import {
  absoluteDirectoryPath,
  artifactRootDirectory,
  type AbsoluteDirectoryPath,
  type ArtifactRootDirectory,
  publishWorkspaceRootDirectory,
  type PublishWorkspaceRootDirectory,
} from "./path-values.ts";
import { renderPublishWorkspace } from "./render-publish-workspace.ts";
import { stagePublishPayloads } from "./stage-publish-payloads.ts";
import { resolveAbsolutePath, type AbsolutePath } from "../sprawl-guard/src/lib/paths.ts";

describe("stagePublishPayloads", () => {
  let tempPublicRoot: AbsoluteDirectoryPath;
  let tempWorkspaceRoot: PublishWorkspaceRootDirectory;
  let tempArtifactRoot: ArtifactRootDirectory;

  beforeEach(() => {
    tempPublicRoot = absoluteDirectoryPath(
      fs.mkdtempSync(
        path.join(os.tmpdir(), "sprawl-guard-public-stage-payloads-workspace-"),
      ),
    );
    tempWorkspaceRoot = publishWorkspaceRootDirectory(path.join(tempPublicRoot, "npm"));
    fs.mkdirSync(tempWorkspaceRoot);
    fs.writeFileSync(path.join(tempPublicRoot, "LICENSE-MIT"), "mit\n");
    fs.writeFileSync(path.join(tempPublicRoot, "LICENSE-APACHE"), "apache\n");
    tempArtifactRoot = artifactRootDirectory(
      fs.mkdtempSync(
        path.join(os.tmpdir(), "sprawl-guard-public-stage-payloads-artifacts-"),
      ),
    );

    fs.mkdirSync(path.join(tempWorkspaceRoot, "sprawl-guard", "dist", "bin"), {
      recursive: true,
    });
    fs.writeFileSync(
      path.join(tempWorkspaceRoot, "sprawl-guard", "README.md"),
      "# wrapper\n",
    );
    fs.writeFileSync(
      path.join(tempWorkspaceRoot, "sprawl-guard", "dist", "bin", "sprawl-guard.js"),
      "console.log('wrapper');\n",
    );

    const renderResult = renderPublishWorkspace({
      workspaceRootDirectory: tempWorkspaceRoot,
    });
    assert.equal(renderResult.kind, "ok");

    for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
      fs.mkdirSync(path.join(tempArtifactRoot, definition.slug), {
        recursive: true,
      });
      const artifactPath = path.join(
        tempArtifactRoot,
        definition.slug,
        definition.artifactRelativePath,
      );
      fs.writeFileSync(
        artifactPath,
        `${definition.slug}:${definition.artifactRelativePath}\n`,
      );
      if (
        definition.artifactRelativePath !== "sprawl-guard.wasm"
        && definition.artifactRelativePath !== "sprawl-guard.exe"
      ) {
        fs.chmodSync(artifactPath, 0o755);
      }
    }
  });

  afterEach(() => {
    fs.rmSync(tempPublicRoot, { recursive: true, force: true });
    fs.rmSync(tempArtifactRoot, { recursive: true, force: true });
  });

  function artifactPathFor(
    definition: (typeof RUNTIME_PACKAGE_DEFINITIONS)[number],
  ): AbsolutePath {
    return resolveAbsolutePath(
      path.join(tempArtifactRoot, definition.slug, definition.artifactRelativePath),
    );
  }

  function destinationPathFor(
    definition: (typeof RUNTIME_PACKAGE_DEFINITIONS)[number],
  ): AbsolutePath {
    return resolveAbsolutePath(
      path.join(
        tempWorkspaceRoot,
        ".stage",
        "packages",
        definition.slug,
        definition.payloadPath,
      ),
    );
  }

  function assertNoRuntimePayloadsWereStaged(): void {
    for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
      assert.equal(fs.existsSync(destinationPathFor(definition)), false);
    }
  }

  function findRuntimePackageDefinition(
    slug: (typeof RUNTIME_PACKAGE_DEFINITIONS)[number]["slug"],
  ): (typeof RUNTIME_PACKAGE_DEFINITIONS)[number] {
    const definition = RUNTIME_PACKAGE_DEFINITIONS.find(
      (candidate) => candidate.slug === slug,
    );
    assert(definition !== undefined);
    return definition;
  }

  describe("when one artifact directory exists for every runtime package slug", () => {
    it("stages every payload into the rendered publish workspace", () => {
      const result = stagePublishPayloads({
        workspaceRootDirectory: tempWorkspaceRoot,
        artifactRootDirectory: tempArtifactRoot,
      });

      assert.equal(result.kind, "ok");
      assert.equal(
        fs.readFileSync(
          path.join(
            tempWorkspaceRoot,
            ".stage",
            "packages",
            "linux-x64-gnu",
            "bin",
            "sprawl-guard",
          ),
          "utf8",
        ),
        "linux-x64-gnu:sprawl-guard\n",
      );
      assert.equal(
        fs.readFileSync(
          path.join(
            tempWorkspaceRoot,
            ".stage",
            "packages",
            "wasm32-wasip1",
            "sprawl-guard.wasm",
          ),
          "utf8",
        ),
        "wasm32-wasip1:sprawl-guard.wasm\n",
      );
    });
  });

  describe("when an artifact payload is a symbolic link", () => {
    it(
      "rejects the source symlink without staging any runtime payloads",
      { skip: process.platform === "win32" },
      () => {
        const definition = findRuntimePackageDefinition("linux-x64-gnu");
        const artifactPath = artifactPathFor(definition);
        const outsideFilePath = path.join(tempArtifactRoot, "outside-sprawl-guard");
        fs.writeFileSync(outsideFilePath, "outside\n");
        fs.rmSync(artifactPath);
        fs.symlinkSync(outsideFilePath, artifactPath);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert(result.kind === "invalid_payload");
        assert.equal(result.relativePath, "linux-x64-gnu/sprawl-guard");
        assert.match(result.detail, /source symbolic links are not allowed/);
        assertNoRuntimePayloadsWereStaged();
      },
    );
  });

  describe("when an artifact payload parent is a symbolic link", () => {
    it(
      "rejects the source parent symlink before reading from outside the artifact root",
      { skip: process.platform === "win32" },
      () => {
        const definition = findRuntimePackageDefinition("linux-x64-gnu");
        const artifactDirectory = path.join(tempArtifactRoot, definition.slug);
        const outsideDirectoryPath = path.join(tempArtifactRoot, "outside-linux-x64-gnu");
        fs.rmSync(artifactDirectory, { recursive: true });
        fs.mkdirSync(outsideDirectoryPath);
        fs.writeFileSync(
          path.join(outsideDirectoryPath, definition.artifactRelativePath),
          "outside\n",
        );
        fs.symlinkSync(outsideDirectoryPath, artifactDirectory);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert(result.kind === "invalid_payload");
        assert.equal(result.relativePath, "linux-x64-gnu/sprawl-guard");
        assert.match(result.detail, /source symbolic links are not allowed/);
        assertNoRuntimePayloadsWereStaged();
      },
    );
  });

  describe("when an artifact payload parent is a dangling symbolic link", () => {
    it(
      "rejects the parent symlink instead of reporting the payload as missing",
      { skip: process.platform === "win32" },
      () => {
        const definition = findRuntimePackageDefinition("linux-x64-gnu");
        const artifactDirectory = path.join(tempArtifactRoot, definition.slug);
        const missingTargetPath = path.join(tempArtifactRoot, "missing-artifact-directory");
        fs.rmSync(artifactDirectory, { recursive: true });
        fs.symlinkSync(missingTargetPath, artifactDirectory);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert(result.kind === "invalid_payload");
        assert.equal(result.relativePath, "linux-x64-gnu/sprawl-guard");
        assert.match(result.detail, /source symbolic links are not allowed/);
        assertNoRuntimePayloadsWereStaged();
      },
    );
  });

  describe("when a native artifact payload is not executable", () => {
    it(
      "rejects the source payload before copying it",
      { skip: process.platform === "win32" },
      () => {
        const definition = findRuntimePackageDefinition("linux-x64-gnu");
        fs.chmodSync(artifactPathFor(definition), 0o644);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert.deepEqual(result, {
          kind: "invalid_payload",
          relativePath: "linux-x64-gnu/sprawl-guard",
          detail: "sprawl-guard (missing executable bit)",
        });
        assertNoRuntimePayloadsWereStaged();
      },
    );
  });

  describe("when a native artifact payload is executable only by its group", () => {
    it(
      "rejects the source payload before copying it",
      { skip: process.platform === "win32" },
      () => {
        const definition = findRuntimePackageDefinition("linux-x64-gnu");
        fs.chmodSync(artifactPathFor(definition), 0o010);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert.deepEqual(result, {
          kind: "invalid_payload",
          relativePath: "linux-x64-gnu/sprawl-guard",
          detail: "sprawl-guard (missing executable bit)",
        });
        assertNoRuntimePayloadsWereStaged();
      },
    );
  });

  describe("when a native artifact payload is executable only by other users", () => {
    it(
      "rejects the source payload before copying it",
      { skip: process.platform === "win32" },
      () => {
        const definition = findRuntimePackageDefinition("linux-x64-gnu");
        fs.chmodSync(artifactPathFor(definition), 0o001);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert.deepEqual(result, {
          kind: "invalid_payload",
          relativePath: "linux-x64-gnu/sprawl-guard",
          detail: "sprawl-guard (missing executable bit)",
        });
        assertNoRuntimePayloadsWereStaged();
      },
    );
  });

  describe("when a destination payload path is already a symbolic link", () => {
    it(
      "rejects the destination symlink without following it",
      { skip: process.platform === "win32" },
      () => {
        const definition = findRuntimePackageDefinition("linux-x64-gnu");
        const destinationPath = destinationPathFor(definition);
        const outsideDirectoryPath = path.join(tempWorkspaceRoot, "outside");
        const outsideFilePath = path.join(outsideDirectoryPath, "sprawl-guard");
        fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
        fs.mkdirSync(outsideDirectoryPath);
        fs.writeFileSync(outsideFilePath, "outside\n");
        fs.symlinkSync(outsideFilePath, destinationPath);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert(result.kind === "invalid_payload");
        assert.equal(result.relativePath, ".stage/packages/linux-x64-gnu/bin/sprawl-guard");
        assert.match(result.detail, /destination symbolic links are not allowed/);
        assert.equal(fs.readFileSync(outsideFilePath, "utf8"), "outside\n");
        assert.equal(fs.lstatSync(destinationPath).isSymbolicLink(), true);
      },
    );
  });

  describe("when a destination payload path is already a hard link", () => {
    it("rejects the destination hard link without overwriting the linked file", () => {
      const definition = findRuntimePackageDefinition("linux-x64-gnu");
      const destinationPath = destinationPathFor(definition);
      const outsideDirectoryPath = path.join(tempWorkspaceRoot, "outside-hard-links");
      const outsideFilePath = path.join(outsideDirectoryPath, "sprawl-guard");
      fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
      fs.mkdirSync(outsideDirectoryPath);
      fs.writeFileSync(outsideFilePath, "outside\n");
      fs.linkSync(outsideFilePath, destinationPath);

      const result = stagePublishPayloads({
        workspaceRootDirectory: tempWorkspaceRoot,
        artifactRootDirectory: tempArtifactRoot,
      });

      assert(result.kind === "invalid_payload");
      assert.equal(result.relativePath, ".stage/packages/linux-x64-gnu/bin/sprawl-guard");
      assert.match(result.detail, /destination hard links are not allowed/);
      assert.equal(fs.readFileSync(outsideFilePath, "utf8"), "outside\n");
      assert.equal(fs.statSync(destinationPath).nlink > 1, true);
    });
  });

  describe("when a destination payload parent is already a symbolic link", () => {
    it(
      "rejects the destination parent symlink without following it",
      { skip: process.platform === "win32" },
      () => {
        const definition = findRuntimePackageDefinition("linux-x64-gnu");
        const packageDirectory = path.join(
          tempWorkspaceRoot,
          ".stage",
          "packages",
          definition.slug,
        );
        const symlinkedBinDirectory = path.join(packageDirectory, "bin");
        const outsideDirectoryPath = path.join(tempWorkspaceRoot, "outside-bin");
        const outsideFilePath = path.join(outsideDirectoryPath, "sprawl-guard");
        fs.mkdirSync(outsideDirectoryPath);
        fs.symlinkSync(outsideDirectoryPath, symlinkedBinDirectory);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert(result.kind === "invalid_payload");
        assert.equal(result.relativePath, ".stage/packages/linux-x64-gnu/bin/sprawl-guard");
        assert.match(result.detail, /destination symbolic links are not allowed/);
        assert.equal(fs.existsSync(outsideFilePath), false);
        assert.equal(fs.lstatSync(symlinkedBinDirectory).isSymbolicLink(), true);
      },
    );
  });

  describe("when one expected artifact is missing", () => {
    it("reports the missing artifact relative path", () => {
      fs.rmSync(path.join(tempArtifactRoot, "linux-x64-musl", "sprawl-guard"));

      const result = stagePublishPayloads({
        workspaceRootDirectory: tempWorkspaceRoot,
        artifactRootDirectory: tempArtifactRoot,
      });

      assert.deepEqual(result, {
        kind: "missing_source_entry",
        relativePath: "linux-x64-musl/sprawl-guard",
      });
    });
  });

  describe("when an artifact path cannot be inspected", () => {
    it("reports the filesystem error instead of treating the artifact as missing", (t) => {
      const inaccessibleDirectory = path.join(tempArtifactRoot, "darwin-arm64");
      const inspectionError = Object.assign(new Error("permission denied"), {
        code: "EACCES",
      });
      const lstatSync = fs.lstatSync;
      t.mock.method(fs, "lstatSync", (entryPath: fs.PathLike) => {
        if (entryPath === inaccessibleDirectory) {
          throw inspectionError;
        }
        return lstatSync(entryPath);
      });

      const result = stagePublishPayloads({
        workspaceRootDirectory: tempWorkspaceRoot,
        artifactRootDirectory: tempArtifactRoot,
      });

      assert.equal(result.kind, "inspection_failed");
      if (result.kind === "inspection_failed") {
        assert.equal(result.relativePath, "darwin-arm64/sprawl-guard");
        assert.equal(result.cause, inspectionError);
      }
      assertNoRuntimePayloadsWereStaged();
    });
  });

  describe("when a rendered workspace prerequisite is a symbolic link", () => {
    it(
      "rejects the prerequisite without following it",
      { skip: process.platform === "win32" },
      () => {
        const manifestPath = path.join(tempWorkspaceRoot, ".stage", "package.json");
        const outsidePath = path.join(tempWorkspaceRoot, "outside-package.json");
        fs.writeFileSync(outsidePath, "{}\n");
        fs.rmSync(manifestPath);
        fs.symlinkSync(outsidePath, manifestPath);

        const result = stagePublishPayloads({
          workspaceRootDirectory: tempWorkspaceRoot,
          artifactRootDirectory: tempArtifactRoot,
        });

        assert(result.kind === "invalid_payload");
        assert.equal(result.relativePath, ".stage/package.json");
        assert.match(result.detail, /destination symbolic links are not allowed/);
        assertNoRuntimePayloadsWereStaged();
      },
    );
  });

  describe("when a rendered workspace prerequisite is a directory", () => {
    it("rejects the prerequisite as the wrong entry type", () => {
      const manifestPath = path.join(tempWorkspaceRoot, ".stage", "package.json");
      fs.rmSync(manifestPath);
      fs.mkdirSync(manifestPath);

      const result = stagePublishPayloads({
        workspaceRootDirectory: tempWorkspaceRoot,
        artifactRootDirectory: tempArtifactRoot,
      });

      assert(result.kind === "invalid_payload");
      assert.equal(result.relativePath, ".stage/package.json");
      assert.match(result.detail, /expected an existing regular file/);
      assertNoRuntimePayloadsWereStaged();
    });
  });

  describe("when the rendered publish workspace is missing", () => {
    it("reports the first missing rendered workspace prerequisite", () => {
      fs.rmSync(path.join(tempWorkspaceRoot, ".stage"), {
        recursive: true,
        force: true,
      });

      const result = stagePublishPayloads({
        workspaceRootDirectory: tempWorkspaceRoot,
        artifactRootDirectory: tempArtifactRoot,
      });

      assert.deepEqual(result, {
        kind: "missing_rendered_entry",
        relativePath: ".stage/package.json",
      });
    });
  });

  describe("when the rendered publish workspace is incomplete", () => {
    it("reports the missing rendered workspace path", () => {
      fs.rmSync(path.join(tempWorkspaceRoot, ".stage", "sprawl-guard", "package.json"));

      const result = stagePublishPayloads({
        workspaceRootDirectory: tempWorkspaceRoot,
        artifactRootDirectory: tempArtifactRoot,
      });

      assert.deepEqual(result, {
        kind: "missing_rendered_entry",
        relativePath: ".stage/sprawl-guard/package.json",
      });
    });
  });
});
