import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  PACKAGE_VERSION,
  RUNTIME_PACKAGE_DEFINITIONS,
  WRAPPER_PACKAGE_NAME,
} from "./package-manifests.ts";
import { NPM_DIST_TAG_SCHEMA } from "./npm-dist-tag.ts";
import {
  type NpmPackageName,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
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
const VERSION_0_3_0 = PUBLIC_PACKAGE_VERSION_SCHEMA.parse("0.3.0");
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

const CARRIER_REQUEST: NpmPublicationRequest = {
  archiveExpectation: {
    kind: "files",
    requiredPaths: [
      PACKAGE_RELATIVE_PATH_SCHEMA.parse(FIRST_RUNTIME_PACKAGE.artifactRelativePath),
    ],
  },
  expectedManifestText: MANIFEST_TEXT,
  expectedPackedManifestText: MANIFEST_TEXT,
  expectedPackedPaths: [
    PACKAGE_RELATIVE_PATH_SCHEMA.parse(FIRST_RUNTIME_PACKAGE.artifactRelativePath),
  ],
  packageDirectory: ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(
    `/release/npm/.stage/packages/${FIRST_RUNTIME_PACKAGE.slug}`,
  ),
  packageName: FIRST_RUNTIME_PACKAGE.packageName,
};
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
  describe("when a complete package set is new", () => {
    it("publishes carriers before the wrapper directly on the selected channel", async () => {
      const events: string[] = [];
      const integrityInspections = new Map<NpmPackageName, number>();
      const published = new Set<NpmPackageName>();
      const operations: NpmPublicationOperations = {
        inspectRemoteDistTag: (packageName) => ({
          kind: "ok",
          remote: published.has(packageName)
            ? { kind: "present", version: PACKAGE_VERSION }
            : { kind: "absent" },
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
          archivePath: archivePath(request.packageName),
          integrity: LOCAL_INTEGRITY,
          packageName: request.packageName,
          version: PACKAGE_VERSION,
        }),
        publishArchive: (archivePath, _integrity, npmDistTag) => {
          const publishedPackage = [CARRIER_REQUEST, WRAPPER_REQUEST].find(
            ({ packageName }) => archivePath === `/release/${packageName}.tgz`,
          );
          assert.ok(publishedPackage !== undefined);
          events.push(`publish:${publishedPackage.packageName}:${npmDistTag}`);
          published.add(publishedPackage.packageName);
          return { kind: "ok" };
        },
        sleepBeforeRetry: async () => {},
      };

      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations,
        requests: [CARRIER_REQUEST, WRAPPER_REQUEST],
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, [
        `publish:${CARRIER_REQUEST.packageName}:beta`,
        `publish:${WRAPPER_PACKAGE_NAME}:beta`,
      ]);
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
        requests: [WRAPPER_REQUEST],
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, ["pack", "inspect"]);
    });
  });

  describe("when a carrier is already published on the selected channel", () => {
    describe("and the wrapper is absent", () => {
      it("publishes only the wrapper", async () => {
        const events: string[] = [];
        const published = new Set<NpmPackageName>([CARRIER_REQUEST.packageName]);
        const wrapperInspections = { count: 0 };
        const operations: NpmPublicationOperations = {
          inspectRemoteDistTag: (packageName) => ({
            kind: "ok",
            remote: published.has(packageName)
              ? { kind: "present", version: PACKAGE_VERSION }
              : { kind: "present", version: VERSION_0_0_9 },
          }),
          inspectRemoteIntegrity: (packageName) => {
            if (packageName === WRAPPER_PACKAGE_NAME) {
              wrapperInspections.count += 1;
            }
            return packageName === WRAPPER_PACKAGE_NAME && wrapperInspections.count < 3
              ? { kind: "ok", integrity: { kind: "absent" } }
              : {
                  kind: "ok",
                  integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
                };
          },
          packPackage: (request) => ({
            kind: "ok",
            archivePath: archivePath(request.packageName),
            integrity: LOCAL_INTEGRITY,
            packageName: request.packageName,
            version: PACKAGE_VERSION,
          }),
          publishArchive: (publishedArchivePath, _integrity, npmDistTag) => {
            assert.equal(publishedArchivePath, archivePath(WRAPPER_PACKAGE_NAME));
            events.push(`publish:${WRAPPER_PACKAGE_NAME}:${npmDistTag}`);
            published.add(WRAPPER_PACKAGE_NAME);
            return { kind: "ok" };
          },
          sleepBeforeRetry: async () => {},
        };

        const result = await publishNpmPublicationRequests({
          npmDistTag: BETA_DIST_TAG,
          operations,
          requests: [CARRIER_REQUEST, WRAPPER_REQUEST],
          temporaryDirectory: RELEASE_DIRECTORY,
        });

        assert.deepEqual(result, { kind: "ok" });
        assert.deepEqual(events, [`publish:${WRAPPER_PACKAGE_NAME}:beta`]);
      });
    });
  });

  describe("when the selected channel already targets a newer release", () => {
    it("stops during preflight without publishing", async () => {
      const events: string[] = [];
      const result = await publishNpmPublicationRequests({
        npmDistTag: BETA_DIST_TAG,
        operations: publicationOperations({
          distTagInspections: [
            { kind: "ok", remote: { kind: "present", version: VERSION_0_3_0 } },
          ],
          events,
          inspections: [{ kind: "ok", integrity: { kind: "absent" } }],
        }),
        requests: [WRAPPER_REQUEST],
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: WRAPPER_PACKAGE_NAME,
        detail: `npm dist-tag beta already targets newer version ${VERSION_0_3_0}`,
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
        requests: [WRAPPER_REQUEST],
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
        requests: [WRAPPER_REQUEST],
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(events, ["pack", "inspect", "sleep"]);
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
        requests: [WRAPPER_REQUEST],
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
        requests: [WRAPPER_REQUEST],
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
            { kind: "ok", remote: { kind: "present", version: VERSION_0_3_0 } },
          ],
          events: [],
          inspections: [
            {
              kind: "ok",
              integrity: { kind: "present", integrity: LOCAL_INTEGRITY },
            },
          ],
        }),
        requests: [WRAPPER_REQUEST],
        temporaryDirectory: RELEASE_DIRECTORY,
      });

      assert.deepEqual(result, {
        kind: "failed",
        packageName: WRAPPER_PACKAGE_NAME,
        detail: `npm dist-tag beta already targets newer version ${VERSION_0_3_0}`,
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
            { kind: "ok", remote: { kind: "present", version: VERSION_0_3_0 } },
          ],
          events,
          inspections: [
            { kind: "ok", integrity: { kind: "absent" } },
            { kind: "ok", integrity: { kind: "absent" } },
          ],
        }),
        requests: [WRAPPER_REQUEST],
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
        requests: [CARRIER_REQUEST, WRAPPER_REQUEST],
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
        requests: [CARRIER_REQUEST, WRAPPER_REQUEST],
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
        requests: [CARRIER_REQUEST],
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
        requests: [WRAPPER_REQUEST],
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
