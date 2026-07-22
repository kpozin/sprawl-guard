import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import {
  prepareReleaseAssets,
  replaceDraftReleaseAssets,
  type GhRunner,
} from "./release-assets.ts";
import {
  expectedReleaseAssetNames,
  parseReleaseTag,
  type ReleaseVersion,
} from "./release-policy.ts";

function validReleaseVersion(value: string): ReleaseVersion {
  const parsed = parseReleaseTag(`v${value}`);
  assert.equal(parsed.kind, "ok");
  return parsed.releaseVersion;
}

function withTemporaryDirectory(run: (directory: string) => void): void {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "sprawl-guard-release-"));
  try {
    run(directory);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

function writeExpectedArchives(directory: string): readonly string[] {
  const archiveNames = expectedReleaseAssetNames(validReleaseVersion("0.1.0")).filter(
    (name) => !name.endsWith("-checksums.txt"),
  );
  for (const name of archiveNames) {
    fs.writeFileSync(path.join(directory, name), `contents of ${name}`);
  }
  return archiveNames;
}

describe("prepareReleaseAssets", () => {
  describe("when every target archive is present", () => {
    it("writes a checksum manifest that covers exactly those archives", () => {
      withTemporaryDirectory((directory) => {
        const archiveNames = writeExpectedArchives(directory);

        assert.deepEqual(prepareReleaseAssets(directory, "0.1.0"), {
          kind: "ok",
          assetNames: expectedReleaseAssetNames(validReleaseVersion("0.1.0")),
        });

        const checksumLines = fs
          .readFileSync(
            path.join(directory, "sprawl-guard-0.1.0-checksums.txt"),
            "utf8",
          )
          .trim()
          .split("\n");
        assert.equal(checksumLines.length, archiveNames.length);
        for (const archiveName of archiveNames) {
          assert.ok(checksumLines.some((line) => line.endsWith(`  ${archiveName}`)));
        }
      });
    });
  });

  describe("when an unexpected archive is present", () => {
    it("rejects the asset directory before generating checksums", () => {
      withTemporaryDirectory((directory) => {
        writeExpectedArchives(directory);
        fs.writeFileSync(path.join(directory, "obsolete.zip"), "stale");

        const result = prepareReleaseAssets(directory, "0.1.0");

        assert.equal(result.kind, "asset_set_mismatch");
      });
    });
  });

  describe("when the command is invoked from the npm workspace", () => {
    it("resolves the repository-root artifact directory through `..`", () => {
      withTemporaryDirectory((repositoryRoot) => {
        const npmDirectory = path.join(repositoryRoot, "npm");
        const assetDirectory = path.join(
          repositoryRoot,
          "downloaded-release-artifacts",
          "github-release-assets",
        );
        fs.mkdirSync(npmDirectory);
        fs.mkdirSync(assetDirectory, { recursive: true });
        writeExpectedArchives(assetDirectory);

        const result = spawnSync(
          process.execPath,
          [
            fileURLToPath(new URL("release-assets.ts", import.meta.url)),
            "prepare",
            "../downloaded-release-artifacts/github-release-assets",
            "0.1.0",
          ],
          { cwd: npmDirectory, encoding: "utf8" },
        );

        assert.equal(result.status, 0, result.stderr);
      });
    });
  });
});

describe("replaceDraftReleaseAssets", () => {
  describe("when the draft contains stale assets", () => {
    it("deletes every old asset, uploads the complete local set, and verifies the result", () => {
      withTemporaryDirectory((directory) => {
        writeExpectedArchives(directory);
        assert.equal(prepareReleaseAssets(directory, "0.1.0").kind, "ok");
        const expectedNames = expectedReleaseAssetNames(validReleaseVersion("0.1.0"));
        const calls: string[][] = [];
        let uploaded = false;
        const runGh: GhRunner = (args) => {
          calls.push([...args]);
          if (args[0] === "release" && args[1] === "upload") {
            uploaded = true;
          }
          if (args[0] === "release" && args[1] === "view") {
            return {
              kind: "ok",
              stdout: JSON.stringify({
                isDraft: true,
                assets:
                  !uploaded
                    ? [{ name: "obsolete.zip" }, { name: expectedNames[0] }]
                    : expectedNames.map((name) => ({ name })),
              }),
            };
          }
          return { kind: "ok", stdout: "" };
        };

        assert.deepEqual(
          replaceDraftReleaseAssets({
            assetDirectory: directory,
            releaseTag: "v0.1.0",
            runGh,
          }),
          { kind: "ok" },
        );
        assert.ok(
          calls.some(
            (args) =>
              args.join(" ")
              === "release delete-asset v0.1.0 --yes -- obsolete.zip",
          ),
        );
        const upload = calls.find(
          (args) => args[0] === "release" && args[1] === "upload",
        );
        assert.deepEqual(
          upload?.slice(3).map((assetPath) => path.basename(assetPath)),
          expectedNames,
        );
      });
    });
  });

  describe("when the release has already been published", () => {
    it("refuses to delete any assets", () => {
      withTemporaryDirectory((directory) => {
        writeExpectedArchives(directory);
        assert.equal(prepareReleaseAssets(directory, "0.1.0").kind, "ok");
        const calls: string[][] = [];
        const runGh: GhRunner = (args) => {
          calls.push([...args]);
          return {
            kind: "ok",
            stdout: JSON.stringify({
              assets: [{ name: "obsolete.zip" }],
              isDraft: false,
            }),
          };
        };

        assert.deepEqual(
          replaceDraftReleaseAssets({
            assetDirectory: directory,
            releaseTag: "v0.1.0",
            runGh,
          }),
          { kind: "release_not_draft", releaseTag: "v0.1.0" },
        );
        assert.equal(calls.length, 1);
      });
    });
  });

  describe("when the release is published after its assets are inspected", () => {
    it("refuses the first deletion", () => {
      withTemporaryDirectory((directory) => {
        writeExpectedArchives(directory);
        assert.equal(prepareReleaseAssets(directory, "0.1.0").kind, "ok");
        const calls: string[][] = [];
        let viewCount = 0;
        const runGh: GhRunner = (args) => {
          calls.push([...args]);
          if (args[0] === "release" && args[1] === "view") {
            viewCount += 1;
            return {
              kind: "ok",
              stdout: JSON.stringify({
                assets: [{ name: "obsolete.zip" }],
                isDraft: viewCount === 1,
              }),
            };
          }
          return { kind: "ok", stdout: "" };
        };

        assert.equal(
          replaceDraftReleaseAssets({
            assetDirectory: directory,
            releaseTag: "v0.1.0",
            runGh,
          }).kind,
          "release_not_draft",
        );
        assert.equal(
          calls.some(
            (args) => args[0] === "release" && args[1] === "delete-asset",
          ),
          false,
        );
      });
    });
  });

  describe("when the uploaded asset set differs from the local set", () => {
    it("reports the mismatch", () => {
      withTemporaryDirectory((directory) => {
        writeExpectedArchives(directory);
        assert.equal(prepareReleaseAssets(directory, "0.1.0").kind, "ok");
        let uploaded = false;
        const runGh: GhRunner = (args) => {
          if (args[0] === "release" && args[1] === "upload") {
            uploaded = true;
          }
          if (args[0] === "release" && args[1] === "view") {
            return {
              kind: "ok",
              stdout: JSON.stringify({
                isDraft: true,
                assets: uploaded ? [{ name: "unexpected.zip" }] : [],
              }),
            };
          }
          return { kind: "ok", stdout: "" };
        };

        assert.equal(
          replaceDraftReleaseAssets({
            assetDirectory: directory,
            releaseTag: "v0.1.0",
            runGh,
          }).kind,
          "asset_set_mismatch",
        );
      });
    });
  });
});
