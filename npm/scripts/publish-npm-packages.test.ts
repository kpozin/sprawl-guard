import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import { gunzipSync, gzipSync } from "node:zlib";
import {
  npmPublicationRequests,
  npmPublishArguments,
  npmStagingDistTag,
  prepareNpmPublicationCandidate,
  publishNpmPublicationRequests,
  verifyNpmPublicationCandidate,
} from "./publish-npm-packages.ts";
import {
  reconcileNpmDistTag,
  reconcileNpmPackageIntegrity,
  type NpmPackageName,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
  type RemoteNpmDistTagInspection,
  type RemotePackageIntegrityInspection,
} from "./npm-publication/model.ts";
import {
  PACKAGE_VERSION,
  RUNTIME_PACKAGE_DEFINITIONS,
  WRAPPER_PACKAGE_NAME,
} from "./package-manifests.ts";
import { SELECTED_NPM_DIST_TAG_SCHEMA } from "./npm-dist-tag.ts";
import {
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  type PublicPackageVersion,
} from "./package-version.ts";
import {
  ABSOLUTE_DIRECTORY_PATH_SCHEMA,
  NPM_ARCHIVE_PATH_SCHEMA,
  NPM_PACKAGE_INTEGRITY_SCHEMA,
  PACKAGE_RELATIVE_PATH_SCHEMA,
} from "./npm-publication/values.ts";
import { parseReleaseSourceExpectation } from "./release-source.ts";

const BETA_DIST_TAG = SELECTED_NPM_DIST_TAG_SCHEMA.parse("beta");
const RELEASE_SOURCE = parseReleaseSourceExpectation(
  ["0123456789abcdef0123456789abcdef01234567", `v${PACKAGE_VERSION}`],
  PACKAGE_VERSION,
);
assert.equal(RELEASE_SOURCE.kind, "ok");
const STAGING_DIST_TAG = npmStagingDistTag(RELEASE_SOURCE.expectation.commit);
const VERSION_0_0_9 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.0.9");
const VERSION_0_2_0 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.2.0");
const VERSION_0_2_0_BETA_1 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.2.0-beta.1");
const VERSION_0_2_0_BETA_2 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.2.0-beta.2");
const VERSION_0_2_0_BETA_3 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.2.0-beta.3");
const RELEASE_DIRECTORY = ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse("/release");
const WORKSPACE_ROOT_DIRECTORY = ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse("/release/npm");
const README_PATH = PACKAGE_RELATIVE_PATH_SCHEMA.parse("README.md");
const MANIFEST_TEXT = "{}\n";
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
      const requests = npmPublicationRequests({ workspaceRootDirectory });

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
  describe("when publishing the manual bootstrap release", () => {
    it("uses the release-scoped staging channel without claiming CI provenance", () => {
      assert.deepEqual(npmPublishArguments(npmArchivePath("package.tgz"), STAGING_DIST_TAG), [
        "publish",
        "/release/package.tgz",
        "--access",
        "public",
        "--ignore-scripts",
        "--tag",
        "sg-candidate-0123456789abcdef0123456789abcdef01234567",
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

const WRAPPER_REQUEST: NpmPublicationRequest = {
  archiveExpectation: {
    kind: "files",
    requiredPaths: [README_PATH],
  },
  expectedManifestText: MANIFEST_TEXT,
  expectedPackedManifestText: MANIFEST_TEXT,
  expectedPackedPaths: [README_PATH],
  packageDirectory: absoluteDirectoryPath("/release/npm/.stage/sprawl-guard"),
  packageName: WRAPPER_PACKAGE_NAME,
};

function runtimePublicationRequest(
  definition: (typeof RUNTIME_PACKAGE_DEFINITIONS)[number],
): NpmPublicationRequest {
  const artifactPath = PACKAGE_RELATIVE_PATH_SCHEMA.parse(
    definition.artifactRelativePath,
  );
  return {
    archiveExpectation: { kind: "files", requiredPaths: [artifactPath] },
    expectedManifestText: MANIFEST_TEXT,
    expectedPackedManifestText: MANIFEST_TEXT,
    expectedPackedPaths: [artifactPath],
    packageDirectory: absoluteDirectoryPath(
      `/release/npm/.stage/packages/${definition.slug}`,
    ),
    packageName: definition.packageName,
  };
}

function publicationOperations(args: {
  readonly distTagInspections?: RemoteNpmDistTagInspection[];
  readonly events: string[];
  readonly inspections: RemotePackageIntegrityInspection[];
  readonly packedName?: NpmPackageName;
  readonly packedVersion?: PublicPackageVersion;
}): NpmPublicationOperations {
  return {
    inspectRemoteDistTag: () =>
      args.distTagInspections?.shift() ?? {
        kind: "ok",
        remote: { kind: "present", version: PACKAGE_VERSION },
      },
    inspectRemoteIntegrity: () => {
      args.events.push("inspect");
      return args.inspections.shift() ?? {
        kind: "permanent_failure",
        detail: "inspection queue exhausted",
      };
    },
    packPackage: () => {
      args.events.push("pack");
      return {
        kind: "ok",
        archivePath: npmArchivePath("package.tgz"),
        integrity: LOCAL_INTEGRITY,
        packageName: args.packedName ?? WRAPPER_PACKAGE_NAME,
        version: args.packedVersion ?? PACKAGE_VERSION,
      };
    },
    publishArchive: () => {
      args.events.push("publish");
      return { kind: "ok" };
    },
    removeRemoteDistTag: () => ({ kind: "ok" }),
    sleepBeforeRetry: async () => {
      args.events.push("sleep");
    },
    updateRemoteDistTag: () => ({ kind: "ok" }),
  };
}

describe("publishNpmPublicationRequests", () => {
  describe("when a later package cannot be assembled", () => {
    it("publishes nothing until every carrier and the wrapper have packed successfully", async () => {
      const events: string[] = [];
      const carrierDefinition = RUNTIME_PACKAGE_DEFINITIONS[0];
      assert.ok(carrierDefinition !== undefined);
      const carrierRequest = runtimePublicationRequest(carrierDefinition);
      const operations: NpmPublicationOperations = {
        inspectRemoteDistTag: () => ({
          kind: "ok",
          remote: { kind: "present", version: PACKAGE_VERSION },
        }),
        inspectRemoteIntegrity: () => {
          events.push("inspect");
          return { kind: "ok", integrity: { kind: "absent" } };
        },
        packPackage: (request) => {
          events.push(`pack:${request.packageName}`);
          return request.packageName === WRAPPER_PACKAGE_NAME
            ? { kind: "failed", detail: "wrapper pack failed" }
            : {
                kind: "ok",
                archivePath: npmArchivePath("carrier.tgz"),
                integrity: LOCAL_INTEGRITY,
                packageName: request.packageName,
                version: PACKAGE_VERSION,
              };
        },
        publishArchive: () => {
          events.push("publish");
          return { kind: "ok" };
        },
        removeRemoteDistTag: () => ({ kind: "ok" }),
        sleepBeforeRetry: async () => {
          events.push("sleep");
        },
        updateRemoteDistTag: () => ({ kind: "ok" }),
      };

      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations,
        requests: [carrierRequest, WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: WRAPPER_PACKAGE_NAME,
        detail: "wrapper pack failed",
      });
      assert.deepEqual(events, [
        `pack:${carrierDefinition.packageName}`,
        `pack:${WRAPPER_PACKAGE_NAME}`,
      ]);
    });
  });

  describe("when a later package conflicts with the registry", () => {
    it("publishes nothing after preflighting every package", async () => {
      const events: string[] = [];
      const carrierDefinition = RUNTIME_PACKAGE_DEFINITIONS[0];
      assert.ok(carrierDefinition !== undefined);
      const carrierRequest = runtimePublicationRequest(carrierDefinition);
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: {
          inspectRemoteDistTag: () => ({
            kind: "ok",
            remote: { kind: "present", version: PACKAGE_VERSION },
          }),
          inspectRemoteIntegrity: (packageName) => {
            events.push(`inspect:${packageName}`);
            return packageName === WRAPPER_PACKAGE_NAME
              ? {
                  kind: "ok",
                  integrity: { kind: "present", integrity: REMOTE_INTEGRITY },
                }
              : { kind: "ok", integrity: { kind: "absent" } };
          },
          packPackage: (request) => {
            events.push(`pack:${request.packageName}`);
            return {
              kind: "ok",
              archivePath: npmArchivePath(`${request.packageName}.tgz`),
              integrity: LOCAL_INTEGRITY,
              packageName: request.packageName,
              version: PACKAGE_VERSION,
            };
          },
          publishArchive: () => {
            events.push("publish");
            return { kind: "ok" };
          },
          removeRemoteDistTag: () => ({ kind: "ok" }),
          sleepBeforeRetry: async () => {},
          updateRemoteDistTag: () => ({ kind: "ok" }),
        },
        requests: [carrierRequest, WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.equal(result.kind, "failed");
      assert.deepEqual(events, [
        `pack:${carrierDefinition.packageName}`,
        `pack:${WRAPPER_PACKAGE_NAME}`,
        `inspect:${carrierDefinition.packageName}`,
        `inspect:${WRAPPER_PACKAGE_NAME}`,
      ]);
    });
  });

  describe("when a complete package set is new", () => {
    it("uploads every archive under the staging tag before advancing the selected channel", async () => {
      const carrierDefinition = RUNTIME_PACKAGE_DEFINITIONS[0];
      assert.ok(carrierDefinition !== undefined);
      const carrierRequest = runtimePublicationRequest(carrierDefinition);
      const events: string[] = [];
      const integrityInspections = new Map<NpmPackageName, number>();
      const selectedTags = new Set<NpmPackageName>();
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: {
          inspectRemoteDistTag: (packageName, npmDistTag) => ({
            kind: "ok",
            remote:
              npmDistTag === STAGING_DIST_TAG || selectedTags.has(packageName)
                ? { kind: "present", version: PACKAGE_VERSION }
                : { kind: "present", version: VERSION_0_0_9 },
          }),
          inspectRemoteIntegrity: (packageName) => {
            const inspection = (integrityInspections.get(packageName) ?? 0) + 1;
            integrityInspections.set(packageName, inspection);
            return inspection < 3
              ? { kind: "ok", integrity: { kind: "absent" } }
              : {
                  kind: "ok",
                  integrity: {
                    kind: "present",
                    integrity: LOCAL_INTEGRITY,
                  },
                };
          },
          packPackage: (request) => ({
            kind: "ok",
            archivePath: npmArchivePath(`${request.packageName}.tgz`),
            integrity: LOCAL_INTEGRITY,
            packageName: request.packageName,
            version: PACKAGE_VERSION,
          }),
          publishArchive: (_archivePath, _integrity, npmDistTag) => {
            events.push(`publish:${npmDistTag}`);
            return { kind: "ok" };
          },
          removeRemoteDistTag: () => ({ kind: "ok" }),
          sleepBeforeRetry: async () => {},
          updateRemoteDistTag: (packageName) => {
            events.push(`update:${packageName}`);
            selectedTags.add(packageName);
            return { kind: "ok" };
          },
        },
        requests: [carrierRequest, WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events.slice(0, 2), [
        `publish:${STAGING_DIST_TAG}`,
        `publish:${STAGING_DIST_TAG}`,
      ]);
      assert.match(events[2] ?? "", /^update:/u);
    });
  });

  describe("when the packed package identity differs from the request", () => {
    it("stops before querying or publishing", async () => {
      const events: string[] = [];
      const differentPackage = RUNTIME_PACKAGE_DEFINITIONS[0];
      assert.ok(differentPackage !== undefined);

      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          events,
          inspections: [],
          packedName: differentPackage.packageName,
        }),
        requests: [WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.equal(result.kind, "failed");
      assert.deepEqual(events, ["pack"]);
    });
  });

  describe("when registry propagation is transient after publication", () => {
    it("retries without repeating the upload", async () => {
      const events: string[] = [];

      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          events,
          inspections: [
            { kind: "ok", integrity: { kind: "absent" } },
            { kind: "ok", integrity: { kind: "absent" } },
            { kind: "transient_failure", detail: "HTTP 503" },
            {
              kind: "ok",
              integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
            },
          ],
        }),
        requests: [WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, [
        "pack",
        "inspect",
        "inspect",
        "publish",
        "inspect",
        "sleep",
        "inspect",
      ]);
    });
  });

  describe("when the package was already published from the same archive", () => {
    it("skips the upload", async () => {
      const events: string[] = [];

      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          events,
          inspections: [
            {
              kind: "ok",
              integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
            },
          ],
        }),
        requests: [WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, ["pack", "inspect"]);
    });
  });

  describe("when the selected channel changes after reconciliation", () => {
    it("retains the staging tag for recovery", async () => {
      const events: string[] = [];
      const operations = publicationOperations({
        distTagInspections: [
          { kind: "ok", remote: { kind: "present", version: PACKAGE_VERSION } },
          { kind: "ok", remote: { kind: "present", version: PACKAGE_VERSION } },
          { kind: "ok", remote: { kind: "present", version: PACKAGE_VERSION } },
          { kind: "ok", remote: { kind: "present", version: VERSION_0_0_9 } },
        ],
        events,
        inspections: [
          {
            kind: "ok",
            integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
          },
        ],
      });
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: {
          ...operations,
          removeRemoteDistTag: () => {
            events.push("remove-tag");
            return { kind: "ok" };
          },
        },
        requests: [WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.equal(result.kind, "failed");
      if (result.kind === "failed") {
        assert.match(result.detail, /selected npm channel .* no longer targets/u);
      }
      assert.equal(events.includes("remove-tag"), false);
    });

    it("reports the drift when the staging tag was already absent", async () => {
      const events: string[] = [];
      const operations = publicationOperations({
        distTagInspections: [
          { kind: "ok", remote: { kind: "present", version: PACKAGE_VERSION } },
          { kind: "ok", remote: { kind: "present", version: PACKAGE_VERSION } },
          { kind: "ok", remote: { kind: "absent" } },
          { kind: "ok", remote: { kind: "present", version: VERSION_0_0_9 } },
        ],
        events,
        inspections: [
          {
            kind: "ok",
            integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
          },
        ],
      });
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations,
        requests: [WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.equal(result.kind, "failed");
      if (result.kind === "failed") {
        assert.match(result.detail, /selected npm channel .* no longer targets/u);
      }
    });
  });

  describe("when a resumed package channel targets an older version", () => {
    it("rereads and advances the channel after the integrity preflight", async () => {
      const events: string[] = [];
      const remoteTags = [
        VERSION_0_0_9,
        VERSION_0_0_9,
        PACKAGE_VERSION,
        PACKAGE_VERSION,
      ];

      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: {
          inspectRemoteDistTag: (_packageName, npmDistTag) => {
            if (npmDistTag === STAGING_DIST_TAG) {
              return { kind: "ok", remote: { kind: "absent" } };
            }
            events.push("inspect-tag");
            const version = remoteTags.shift();
            return version === undefined
              ? { kind: "permanent_failure", detail: "tag queue exhausted" }
              : { kind: "ok", remote: { kind: "present", version } };
          },
          inspectRemoteIntegrity: () => {
            events.push("inspect-integrity");
            return {
              kind: "ok",
              integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
            };
          },
          packPackage: () => {
            events.push("pack");
            return {
              kind: "ok",
              archivePath: npmArchivePath("package.tgz"),
              integrity: LOCAL_INTEGRITY,
              packageName: WRAPPER_PACKAGE_NAME,
              version: PACKAGE_VERSION,
            };
          },
          publishArchive: () => {
            events.push("publish");
            return { kind: "ok" };
          },
          removeRemoteDistTag: () => ({ kind: "ok" }),
          sleepBeforeRetry: async () => {
            events.push("sleep");
          },
          updateRemoteDistTag: () => {
            events.push("update-tag");
            return { kind: "ok" };
          },
        },
        requests: [WRAPPER_REQUEST],
        stagingNpmDistTag: STAGING_DIST_TAG,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, [
        "pack",
        "inspect-integrity",
        "inspect-tag",
        "inspect-tag",
        "update-tag",
        "inspect-tag",
        "inspect-tag",
      ]);
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
        for (const request of requests) {
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
            npmDistTag: SELECTED_NPM_DIST_TAG_SCHEMA.parse("latest"),
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
