import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  PACKAGE_VERSION,
  RUNTIME_PACKAGE_DEFINITIONS,
  WRAPPER_PACKAGE_NAME,
} from "./package-manifests.ts";
import { NPM_DIST_TAG_SCHEMA } from "./npm-dist-tag.ts";
import {
  releasePackageList,
  type NpmPackageName,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
  type NpmReleasePackages,
  type RemoteNpmDistTagInspection,
  type RemotePackageIntegrityInspection,
} from "./npm-publication/model.ts";
import { publishNpmPublicationRequests } from "./npm-publication/state.ts";
import {
  ABSOLUTE_DIRECTORY_PATH_SCHEMA,
  NPM_ARCHIVE_PATH_SCHEMA,
  NPM_PACKAGE_INTEGRITY_SCHEMA,
  PACKAGE_RELATIVE_PATH_SCHEMA,
} from "./npm-publication/values.ts";
import { PUBLIC_PACKAGE_VERSION_SCHEMA } from "./package-version.ts";

const BETA_DIST_TAG = NPM_DIST_TAG_SCHEMA.parse("beta");
const VERSION_0_0_9 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.0.9");
/** A version newer than any release, so the tests hold whatever the package version is. */
const NEWER_VERSION = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("999.0.0");
const FIRST_RUNTIME_PACKAGE = RUNTIME_PACKAGE_DEFINITIONS[0];
assert.ok(FIRST_RUNTIME_PACKAGE !== undefined);
const RELEASE_DIRECTORY = ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse("/release");
const MANIFEST_TEXT = "{}\n";
const README_PATH = PACKAGE_RELATIVE_PATH_SCHEMA.parse("README.md");
const LOCAL_INTEGRITY = NPM_PACKAGE_INTEGRITY_SCHEMA.parse(
  "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
);
const CONFLICTING_INTEGRITY = NPM_PACKAGE_INTEGRITY_SCHEMA.parse(
  "sha512-BAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
);

function archivePath(packageName: NpmPackageName) {
  return NPM_ARCHIVE_PATH_SCHEMA.parse(`/release/${packageName}.tgz`);
}

describe("npm publication value types", () => {
  it("accepts valid values at their external boundaries", () => {
    assert.equal(ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse("/release/npm"), "/release/npm");
    assert.equal(NPM_ARCHIVE_PATH_SCHEMA.parse("/release/package.tgz"), "/release/package.tgz");
    assert.equal(
      NPM_PACKAGE_INTEGRITY_SCHEMA.parse(
        "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
      ),
      "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
    );
    assert.equal(PACKAGE_RELATIVE_PATH_SCHEMA.parse("bin/sprawl-guard"), "bin/sprawl-guard");
  });

  it("rejects values from a different string domain", () => {
    assert.throws(() => ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse("release/npm"));
    assert.throws(() => NPM_ARCHIVE_PATH_SCHEMA.parse("/release/package.zip"));
    assert.throws(() => NPM_PACKAGE_INTEGRITY_SCHEMA.parse("sha512-local"));
    assert.throws(() => PACKAGE_RELATIVE_PATH_SCHEMA.parse("../package.json"));
  });
});

function platformRequest(
  definition: (typeof RUNTIME_PACKAGE_DEFINITIONS)[number],
): NpmPublicationRequest {
  const payloadPath = PACKAGE_RELATIVE_PATH_SCHEMA.parse(definition.artifactRelativePath);
  return {
    archiveExpectation: { kind: "files", requiredPaths: [payloadPath] },
    expectedManifestText: MANIFEST_TEXT,
    expectedPackedManifestText: MANIFEST_TEXT,
    expectedPackedPaths: [payloadPath],
    packageDirectory: ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(
      `/release/npm/.stage/packages/${definition.slug}`,
    ),
    packageName: definition.packageName,
  };
}

const CARRIER_REQUEST = platformRequest(FIRST_RUNTIME_PACKAGE);
const WRAPPER_REQUEST: NpmPublicationRequest = {
  archiveExpectation: { kind: "files", requiredPaths: [README_PATH] },
  expectedManifestText: MANIFEST_TEXT,
  expectedPackedManifestText: MANIFEST_TEXT,
  expectedPackedPaths: [README_PATH],
  packageDirectory: ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(
    "/release/npm/.stage/sprawl-guard",
  ),
  packageName: WRAPPER_PACKAGE_NAME,
};
const WRAPPER_ONLY: NpmReleasePackages<NpmPublicationRequest> = {
  platformPackages: [],
  wrapper: WRAPPER_REQUEST,
};
const ONE_CARRIER_AND_WRAPPER: NpmReleasePackages<NpmPublicationRequest> = {
  platformPackages: [CARRIER_REQUEST],
  wrapper: WRAPPER_REQUEST,
};
const FULL_RELEASE: NpmReleasePackages<NpmPublicationRequest> = {
  platformPackages: RUNTIME_PACKAGE_DEFINITIONS.map(platformRequest),
  wrapper: WRAPPER_REQUEST,
};
const PLATFORM_PACKAGE_NAMES = FULL_RELEASE.platformPackages.map(
  ({ packageName }) => packageName,
);
const LAST_RUNTIME_PACKAGE = RUNTIME_PACKAGE_DEFINITIONS.at(-1);
assert.ok(LAST_RUNTIME_PACKAGE !== undefined);

/**
 * A fake registry that serves each package as soon as it is uploaded. It records each upload
 * (`upload:`) and each time a post-upload wait sees a package (`served:`), so tests can check the
 * order of the phases.
 */
function simulatedRegistry(args: {
  readonly alreadyPublished?: readonly NpmPackageName[];
  readonly failsUploadOf?: NpmPackageName;
  readonly servesConflictingContentsOf?: NpmPackageName;
}) {
  const events: string[] = [];
  const servedPackages = new Set<NpmPackageName>(args.alreadyPublished);
  const uploadedPackages = new Set<NpmPackageName>();
  const packageNameOf = (path: string): NpmPackageName => {
    const request = releasePackageList(FULL_RELEASE).find(
      ({ packageName }) => path === archivePath(packageName),
    );
    assert.ok(request !== undefined);
    return request.packageName;
  };
  const operations: NpmPublicationOperations = {
    inspectRemoteDistTag: (packageName) => ({
      kind: "ok",
      remote: servedPackages.has(packageName)
        ? { kind: "present", version: PACKAGE_VERSION }
        : { kind: "present", version: VERSION_0_0_9 },
    }),
    inspectRemoteIntegrity: (packageName) => {
      if (!servedPackages.has(packageName)) {
        return { kind: "ok", integrity: { kind: "absent" } };
      }
      if (uploadedPackages.has(packageName)) {
        events.push(`served:${packageName}`);
      }
      return {
        kind: "ok",
        integrity: {
          kind: "present",
          integrity: packageName === args.servesConflictingContentsOf
            ? CONFLICTING_INTEGRITY
            : LOCAL_INTEGRITY,
        },
      };
    },
    packPackage: (request) => ({
      kind: "ok",
      archivePath: archivePath(request.packageName),
      integrity: LOCAL_INTEGRITY,
      packageName: request.packageName,
      version: PACKAGE_VERSION,
    }),
    publishArchive: (path, _integrity, npmDistTag) => {
      const packageName = packageNameOf(path);
      assert.equal(npmDistTag, BETA_DIST_TAG);
      events.push(`upload:${packageName}`);
      if (packageName === args.failsUploadOf) {
        return { kind: "failed", detail: "npm publish exited with status 1" };
      }
      servedPackages.add(packageName);
      uploadedPackages.add(packageName);
      return { kind: "ok" };
    },
    sleepBeforeRetry: async () => {},
  };
  return { events, operations };
}

function eventsWithPrefix(events: readonly string[], prefix: string): string[] {
  return events.filter((event) => event.startsWith(prefix));
}

function publicationOperations(args: {
  readonly distTagInspections?: RemoteNpmDistTagInspection[];
  readonly events: string[];
  readonly inspections: RemotePackageIntegrityInspection[];
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
        archivePath: archivePath(WRAPPER_PACKAGE_NAME),
        integrity: LOCAL_INTEGRITY,
        packageName: WRAPPER_PACKAGE_NAME,
        version: PACKAGE_VERSION,
      };
    },
    publishArchive: () => {
      args.events.push("publish");
      return { kind: "ok" };
    },
    sleepBeforeRetry: async () => {},
  };
}

describe("publishNpmPublicationRequests", () => {
  describe("when every platform package and the wrapper are new", () => {
    async function publishFullRelease() {
      const registry = simulatedRegistry({});
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: registry.operations,
        requests: FULL_RELEASE,
        temporaryDirectory: RELEASE_DIRECTORY,
      });
      return { ...registry, result };
    }

    it("succeeds", async () => {
      const { result } = await publishFullRelease();
      assert.deepEqual(result, { kind: "ok" });
    });

    it("starts the platform uploads in release order and the wrapper upload last", async () => {
      const { events } = await publishFullRelease();
      assert.deepEqual(eventsWithPrefix(events, "upload:"), [
        ...PLATFORM_PACKAGE_NAMES.map((packageName) => `upload:${packageName}`),
        `upload:${WRAPPER_PACKAGE_NAME}`,
      ]);
    });

    it("uploads every platform package before waiting for any package", async () => {
      const { events } = await publishFullRelease();
      const lastPlatformUpload = Math.max(
        ...PLATFORM_PACKAGE_NAMES.map((packageName) =>
          events.indexOf(`upload:${packageName}`),
        ),
      );
      assert.ok(lastPlatformUpload < events.findIndex((event) => event.startsWith("served:")));
    });

    it("uploads the wrapper only after npm serves every platform package", async () => {
      const { events } = await publishFullRelease();
      const wrapperUpload = events.indexOf(`upload:${WRAPPER_PACKAGE_NAME}`);
      for (const packageName of PLATFORM_PACKAGE_NAMES) {
        const served = events.indexOf(`served:${packageName}`);
        assert.ok(served !== -1 && served < wrapperUpload, `${packageName} served first`);
      }
      assert.equal(events.at(-1), `served:${WRAPPER_PACKAGE_NAME}`);
    });
  });

  describe("when some platform packages are already published", () => {
    it("uploads and waits for only the absent packages", async () => {
      const registry = simulatedRegistry({
        alreadyPublished: [FIRST_RUNTIME_PACKAGE.packageName],
      });
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: registry.operations,
        requests: FULL_RELEASE,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      const absentPackageNames = [
        ...PLATFORM_PACKAGE_NAMES.slice(1),
        WRAPPER_PACKAGE_NAME,
      ];
      assert.deepEqual(
        eventsWithPrefix(registry.events, "upload:"),
        absentPackageNames.map((packageName) => `upload:${packageName}`),
      );
      assert.deepEqual(
        eventsWithPrefix(registry.events, "served:"),
        absentPackageNames.map((packageName) => `served:${packageName}`),
      );
    });
  });

  describe("when a platform upload fails", () => {
    async function publishWithFailedUpload() {
      const registry = simulatedRegistry({
        failsUploadOf: FIRST_RUNTIME_PACKAGE.packageName,
      });
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: registry.operations,
        requests: FULL_RELEASE,
        temporaryDirectory: RELEASE_DIRECTORY,
      });
      return { ...registry, result };
    }

    it("reports the failed platform package", async () => {
      const { result } = await publishWithFailedUpload();
      assert.deepEqual(result, {
        kind: "failed",
        packageName: FIRST_RUNTIME_PACKAGE.packageName,
        detail: "npm publish exited with status 1",
      });
    });

    it("starts no further upload", async () => {
      const { events } = await publishWithFailedUpload();
      assert.deepEqual(eventsWithPrefix(events, "upload:"), [
        `upload:${FIRST_RUNTIME_PACKAGE.packageName}`,
      ]);
    });

    it("waits for no package", async () => {
      const { events } = await publishWithFailedUpload();
      assert.deepEqual(eventsWithPrefix(events, "served:"), []);
    });
  });

  describe("when npm serves different contents for a platform package", () => {
    it("reports that package without uploading the wrapper", async () => {
      const registry = simulatedRegistry({
        servesConflictingContentsOf: LAST_RUNTIME_PACKAGE.packageName,
      });
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: registry.operations,
        requests: FULL_RELEASE,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: LAST_RUNTIME_PACKAGE.packageName,
        detail: `published integrity ${CONFLICTING_INTEGRITY} differs from uploaded integrity ${LOCAL_INTEGRITY}`,
      });
      assert.equal(registry.events.includes(`upload:${WRAPPER_PACKAGE_NAME}`), false);
    });
  });

  describe("when the wrapper upload fails", () => {
    it("reports the wrapper after npm serves every platform package", async () => {
      const registry = simulatedRegistry({ failsUploadOf: WRAPPER_PACKAGE_NAME });
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: registry.operations,
        requests: FULL_RELEASE,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: WRAPPER_PACKAGE_NAME,
        detail: "npm publish exited with status 1",
      });
      assert.deepEqual(
        eventsWithPrefix(registry.events, "served:"),
        PLATFORM_PACKAGE_NAMES.map((packageName) => `served:${packageName}`),
      );
    });
  });

  describe("when npm serves different contents for the wrapper", () => {
    it("reports the wrapper", async () => {
      const registry = simulatedRegistry({
        servesConflictingContentsOf: WRAPPER_PACKAGE_NAME,
      });
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: registry.operations,
        requests: FULL_RELEASE,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: WRAPPER_PACKAGE_NAME,
        detail: `published integrity ${CONFLICTING_INTEGRITY} differs from uploaded integrity ${LOCAL_INTEGRITY}`,
      });
    });
  });

  describe("when an identical package version already targets the selected channel", () => {
    it("resumes without publishing or mutating a dist-tag", async () => {
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
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, ["pack", "inspect"]);
    });
  });

  describe("when the selected channel already targets a newer release", () => {
    it("stops during preflight without publishing", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          distTagInspections: [
            { kind: "ok", remote: { kind: "present", version: NEWER_VERSION } },
          ],
          events,
          inspections: [{ kind: "ok", integrity: { kind: "absent" } }],
        }),
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: WRAPPER_PACKAGE_NAME,
        detail: `npm dist-tag beta already targets newer version ${NEWER_VERSION}`,
      });
      assert.deepEqual(events, ["pack", "inspect"]);
    });
  });

  describe("when an npm preflight read fails transiently", () => {
    it("retries package inspection before publishing", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: {
          ...publicationOperations({
            events,
            inspections: [
              { kind: "transient_failure", detail: "HTTP 429" },
              { kind: "ok", integrity: { kind: "absent" } },
              { kind: "ok", integrity: { kind: "absent" } },
              {
                kind: "ok",
                integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
              },
            ],
          }),
          sleepBeforeRetry: async () => {
            events.push("sleep");
          },
        },
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, [
        "pack",
        "inspect",
        "sleep",
        "inspect",
        "inspect",
        "publish",
        "inspect",
      ]);
    });

    it("retries selected-channel inspection before resuming", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: {
          ...publicationOperations({
            distTagInspections: [
              { kind: "transient_failure", detail: "HTTP 429" },
              { kind: "ok", remote: { kind: "present", version: PACKAGE_VERSION } },
            ],
            events,
            inspections: [
              {
                kind: "ok",
                integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
              },
            ],
          }),
          sleepBeforeRetry: async () => {
            events.push("sleep");
          },
        },
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, ["pack", "inspect", "sleep"]);
    });
  });

  describe("when npm takes several minutes to serve a new version", () => {
    it("keeps waiting after the transient retry budget runs out", async () => {
      const events: string[] = [];
      const slowVisibilityPolls = 150;
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          events,
          inspections: [
            ...Array.from(
              { length: slowVisibilityPolls + 2 },
              (): RemotePackageIntegrityInspection => ({
                kind: "ok",
                integrity: { kind: "absent" },
              }),
            ),
            { kind: "ok", integrity: { kind: "present", integrity: LOCAL_INTEGRITY } },
          ],
        }),
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
    });
  });

  describe("when an identical version does not own the selected channel", () => {
    it("stops with a repair command instead of changing the channel", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          distTagInspections: [
            { kind: "ok", remote: { kind: "present", version: VERSION_0_0_9 } },
          ],
          events,
          inspections: [
            {
              kind: "ok",
              integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
            },
          ],
        }),
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.equal(result.kind, "failed");
      if (result.kind === "failed") {
        assert.ok(
          result.detail.includes(
            `npm dist-tag add @kpozin/sprawl-guard@${PACKAGE_VERSION} beta --registry https://registry.npmjs.org/`,
          ),
        );
      }
      assert.deepEqual(events, ["pack", "inspect"]);
    });
  });

  describe("when an identical version loses the selected channel after preflight", () => {
    it("fails instead of reporting a completed publication", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          distTagInspections: [
            { kind: "ok", remote: { kind: "present", version: PACKAGE_VERSION } },
            { kind: "ok", remote: { kind: "absent" } },
          ],
          events,
          inspections: [
            {
              kind: "ok",
              integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
            },
          ],
        }),
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.equal(result.kind, "failed");
      assert.deepEqual(events, ["pack", "inspect"]);
    });
  });

  describe("when a newer release takes the channel from an identical version after preflight", () => {
    it("refuses without suggesting a rollback", async () => {
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          distTagInspections: [
            { kind: "ok", remote: { kind: "present", version: PACKAGE_VERSION } },
            { kind: "ok", remote: { kind: "present", version: NEWER_VERSION } },
          ],
          events: [],
          inspections: [
            {
              kind: "ok",
              integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
            },
          ],
        }),
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: WRAPPER_PACKAGE_NAME,
        detail: `npm dist-tag beta already targets newer version ${NEWER_VERSION}`,
      });
    });
  });

  describe("when the selected channel advances after preflight", () => {
    it("refuses to publish an older package onto the channel", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          distTagInspections: [
            { kind: "ok", remote: { kind: "present", version: VERSION_0_0_9 } },
            { kind: "ok", remote: { kind: "present", version: NEWER_VERSION } },
          ],
          events,
          inspections: [
            { kind: "ok", integrity: { kind: "absent" } },
            { kind: "ok", integrity: { kind: "absent" } },
          ],
        }),
        requests: WRAPPER_ONLY,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.equal(result.kind, "failed");
      assert.equal(events.includes("publish"), false);
    });
  });

  describe("when a later package conflicts with the registry", () => {
    it("finishes the complete preflight before publishing a carrier", async () => {
      const events: string[] = [];
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
                  integrity: { kind: "present", integrity: CONFLICTING_INTEGRITY },
                }
              : { kind: "ok", integrity: { kind: "absent" } };
          },
          packPackage: (request) => {
            events.push(`pack:${request.packageName}`);
            return {
              kind: "ok",
              archivePath: archivePath(request.packageName),
              integrity: LOCAL_INTEGRITY,
              packageName: request.packageName,
              version: PACKAGE_VERSION,
            };
          },
          publishArchive: () => {
            events.push("publish");
            return { kind: "ok" };
          },
          sleepBeforeRetry: async () => {},
        },
        requests: ONE_CARRIER_AND_WRAPPER,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.equal(result.kind, "failed");
      assert.deepEqual(events, [
        `pack:${CARRIER_REQUEST.packageName}`,
        `pack:${WRAPPER_PACKAGE_NAME}`,
        `inspect:${CARRIER_REQUEST.packageName}`,
        `inspect:${WRAPPER_PACKAGE_NAME}`,
      ]);
    });
  });

  describe("when a later package cannot be assembled", () => {
    it("publishes nothing until every carrier and the wrapper have packed", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: {
          ...publicationOperations({ events, inspections: [] }),
          packPackage: (request) => {
            events.push(`pack:${request.packageName}`);
            return request.packageName === WRAPPER_PACKAGE_NAME
              ? { kind: "failed", detail: "wrapper pack failed" }
              : {
                  kind: "ok",
                  archivePath: archivePath(request.packageName),
                  integrity: LOCAL_INTEGRITY,
                  packageName: request.packageName,
                  version: PACKAGE_VERSION,
                };
          },
        },
        requests: ONE_CARRIER_AND_WRAPPER,
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: WRAPPER_PACKAGE_NAME,
        detail: "wrapper pack failed",
      });
      assert.deepEqual(events, [
        `pack:${CARRIER_REQUEST.packageName}`,
        `pack:${WRAPPER_PACKAGE_NAME}`,
      ]);
    });
  });

  describe("when the packed package identity differs from the request", () => {
    it("stops before querying or publishing", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({ events, inspections: [] }),
        requests: ONE_CARRIER_AND_WRAPPER,
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
        operations: {
          ...publicationOperations({
            distTagInspections: [
              { kind: "ok", remote: { kind: "absent" } },
              { kind: "ok", remote: { kind: "absent" } },
            ],
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
          sleepBeforeRetry: async () => {
            events.push("sleep");
          },
        },
        requests: WRAPPER_ONLY,
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
});
