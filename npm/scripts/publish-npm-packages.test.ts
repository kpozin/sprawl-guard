import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import { gunzipSync, gzipSync } from "node:zlib";
import {
  npmPublicationRequests,
  npmPublishArguments,
  prepareNpmPublicationCandidate,
  verifyNpmPublicationCandidate,
} from "./publish-npm-packages.ts";
import {
  reconcileNpmDistTag,
  reconcileNpmPackageIntegrity,
  releasePackageList,
} from "./npm-publication/model.ts";
import {
  PACKAGE_VERSION,
  RUNTIME_PACKAGE_DEFINITIONS,
  WRAPPER_PACKAGE_NAME,
} from "./package-manifests.ts";
import { NPM_DIST_TAG_SCHEMA } from "./npm-dist-tag.ts";
import { PUBLIC_PACKAGE_VERSION_SCHEMA } from "./package-version.ts";
import {
  ABSOLUTE_DIRECTORY_PATH_SCHEMA,
  NPM_ARCHIVE_PATH_SCHEMA,
  NPM_PACKAGE_INTEGRITY_SCHEMA,
} from "./npm-publication/values.ts";
import { parseReleaseSourceExpectation } from "./release-source.ts";

const BETA_DIST_TAG = NPM_DIST_TAG_SCHEMA.parse("beta");
const VERSION_0_2_0 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.2.0");
const VERSION_0_2_0_BETA_1 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.2.0-beta.1");
const VERSION_0_2_0_BETA_2 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.2.0-beta.2");
const VERSION_0_2_0_BETA_3 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.2.0-beta.3");
const RELEASE_DIRECTORY = ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse("/release");
const WORKSPACE_ROOT_DIRECTORY = ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse("/release/npm");
const LOCAL_INTEGRITY = npmPackageIntegrity("A");
const REMOTE_INTEGRITY = npmPackageIntegrity("B");

function npmPackageIntegrity(base64Character: string) {
  return NPM_PACKAGE_INTEGRITY_SCHEMA.parse(
    `sha512-${base64Character.repeat(86)}==`,
  );
}

function npmArchivePath(fileName: string) {
  return NPM_ARCHIVE_PATH_SCHEMA.parse(path.join(RELEASE_DIRECTORY, fileName));
}

function absoluteDirectoryPath(value: string) {
  return ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(value);
}

describe("npmPublicationRequests", () => {
  describe("when deriving the first-release publication order", () => {
    it("publishes every runtime carrier before the wrapper", () => {
      const workspaceRootDirectory = WORKSPACE_ROOT_DIRECTORY;
      const requests = releasePackageList(npmPublicationRequests({ workspaceRootDirectory }));

      assert.deepEqual(
        requests.map(({ packageName }) => packageName),
        [
          ...RUNTIME_PACKAGE_DEFINITIONS.map(({ packageName }) => packageName),
          WRAPPER_PACKAGE_NAME,
        ],
      );
      assert.deepEqual(
        requests.map(({ packageDirectory }) => packageDirectory),
        [
          ...RUNTIME_PACKAGE_DEFINITIONS.map(({ slug }) =>
            path.join(workspaceRootDirectory, ".stage", "packages", slug),
          ),
          path.join(workspaceRootDirectory, ".stage", "sprawl-guard"),
        ],
      );
    });
  });
});

describe("npmPublishArguments", () => {
  describe("when publishing one archive", () => {
    it("publishes it publicly and directly on the selected channel", () => {
      assert.deepEqual(npmPublishArguments(npmArchivePath("package.tgz"), BETA_DIST_TAG), [
        "publish",
        "/release/package.tgz",
        "--access",
        "public",
        "--ignore-scripts",
        "--tag",
        "beta",
        "--registry",
        "https://registry.npmjs.org/",
      ]);
    });
  });
});

describe("reconcileNpmDistTag", () => {
  describe("when the selected channel does not exist", () => {
    it("requests a channel update", () => {
      assert.deepEqual(
        reconcileNpmDistTag({
          remote: { kind: "absent" },
          releaseVersion: VERSION_0_2_0,
        }),
        { kind: "update" },
      );
    });
  });

  describe("when the selected channel targets the release", () => {
    it("keeps the current target", () => {
      assert.deepEqual(
        reconcileNpmDistTag({
          remote: { kind: "present", version: VERSION_0_2_0_BETA_2 },
          releaseVersion: VERSION_0_2_0_BETA_2,
        }),
        { kind: "keep" },
      );
    });
  });

  describe("when the selected channel targets an older release", () => {
    it("requests a channel update", () => {
      assert.deepEqual(
        reconcileNpmDistTag({
          remote: { kind: "present", version: VERSION_0_2_0_BETA_1 },
          releaseVersion: VERSION_0_2_0_BETA_2,
        }),
        { kind: "update" },
      );
    });
  });

  describe("when the selected channel targets a newer release", () => {
    it("refuses to move the channel backward", () => {
      assert.deepEqual(
        reconcileNpmDistTag({
          remote: { kind: "present", version: VERSION_0_2_0_BETA_3 },
          releaseVersion: VERSION_0_2_0_BETA_2,
        }),
        { kind: "would_move_backward", currentVersion: "0.2.0-beta.3" },
      );
    });
  });
});

describe("reconcileNpmPackageIntegrity", () => {
  describe("when the package version is absent", () => {
    it("requests publication", () => {
      assert.deepEqual(
        reconcileNpmPackageIntegrity({
          localIntegrity: LOCAL_INTEGRITY,
          remoteIntegrity: { kind: "absent" },
        }),
        { kind: "publish" },
      );
    });
  });

  describe("when the published tarball matches the staged tarball", () => {
    it("allows a resumed release to skip the package", () => {
      assert.deepEqual(
        reconcileNpmPackageIntegrity({
          localIntegrity: LOCAL_INTEGRITY,
          remoteIntegrity: {
            kind: "present",
            integrity: LOCAL_INTEGRITY,
          },
        }),
        { kind: "skip" },
      );
    });
  });

  describe("when the published tarball differs from the staged tarball", () => {
    it("stops rather than overwriting an immutable version", () => {
      assert.deepEqual(
        reconcileNpmPackageIntegrity({
          localIntegrity: LOCAL_INTEGRITY,
          remoteIntegrity: {
            kind: "present",
            integrity: REMOTE_INTEGRITY,
          },
        }),
        {
          kind: "integrity_mismatch",
          localIntegrity: LOCAL_INTEGRITY,
          remoteIntegrity: REMOTE_INTEGRITY,
        },
      );
    });
  });
});

describe("npm publication candidate", () => {
  describe("when every assembled package matches generated metadata", () => {
    it("packs and verifies all eight immutable archives", () => {
      const temporaryDirectory = fs.mkdtempSync(
        path.join(os.tmpdir(), "sprawl-guard-npm-candidate-test-"),
      );
      try {
        const workspaceRootDirectory = absoluteDirectoryPath(
          path.join(temporaryDirectory, "npm"),
        );
        const candidateDirectory = absoluteDirectoryPath(
          path.join(temporaryDirectory, "candidate"),
        );
        fs.mkdirSync(workspaceRootDirectory, { recursive: true });
        fs.writeFileSync(
          path.join(workspaceRootDirectory, "package.json"),
          '{"private":true,"packageManager":"pnpm@11.28.11"}\n',
        );
        const requests = npmPublicationRequests({ workspaceRootDirectory });
        for (const request of releasePackageList(requests)) {
          for (const relativePath of request.expectedPackedPaths) {
            const filePath = path.join(request.packageDirectory, relativePath);
            fs.mkdirSync(path.dirname(filePath), { recursive: true });
            fs.writeFileSync(
              filePath,
              relativePath === "package.json"
                ? request.expectedManifestText
                : `${relativePath}\n`,
            );
          }
          if (request.archiveExpectation.kind === "posix_executable") {
            fs.chmodSync(
              path.join(
                request.packageDirectory,
                request.archiveExpectation.executablePath,
              ),
              0o755,
            );
          }
        }
        const parsedExpectation = parseReleaseSourceExpectation([
          "0123456789abcdef0123456789abcdef01234567",
          `v${PACKAGE_VERSION}`,
        ], PACKAGE_VERSION);
        assert.equal(parsedExpectation.kind, "ok");
        if (parsedExpectation.kind !== "ok") {
          throw new Error("test release expectation must parse");
        }

        assert.deepEqual(
          prepareNpmPublicationCandidate({
            expectation: parsedExpectation.expectation,
            npmDistTag: BETA_DIST_TAG,
            outputDirectory: candidateDirectory,
            workspaceRootDirectory,
          }),
          { kind: "ok" },
        );
        assert.deepEqual(
          verifyNpmPublicationCandidate({
            candidateDirectory,
            expectation: parsedExpectation.expectation,
            npmDistTag: BETA_DIST_TAG,
            workspaceRootDirectory,
          }),
          { kind: "ok" },
        );
        assert.equal(
          fs.readdirSync(candidateDirectory).filter((file) => file.endsWith(".tgz"))
            .length,
          8,
        );
        assert.deepEqual(
          verifyNpmPublicationCandidate({
            candidateDirectory,
            expectation: parsedExpectation.expectation,
            npmDistTag: NPM_DIST_TAG_SCHEMA.parse("latest"),
            workspaceRootDirectory,
          }).kind,
          "failed",
        );
        const archiveFile = fs
          .readdirSync(candidateDirectory)
          .find((file) => file.endsWith(".tgz"));
        assert.ok(archiveFile !== undefined);
        const archivePath = path.join(candidateDirectory, archiveFile);
        fs.writeFileSync(
          archivePath,
          gzipSync(gunzipSync(fs.readFileSync(archivePath)), { level: 1 }),
        );
        const tampered = verifyNpmPublicationCandidate({
          candidateDirectory,
          expectation: parsedExpectation.expectation,
          npmDistTag: BETA_DIST_TAG,
          workspaceRootDirectory,
        });
        assert.equal(tampered.kind, "failed");
        if (tampered.kind === "failed") {
          assert.match(tampered.detail, /archive hash differs/u);
        }
      } finally {
        fs.rmSync(temporaryDirectory, { force: true, recursive: true });
      }
    });
  });
});
