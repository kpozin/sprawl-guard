#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { gt } from "semver";
import { z } from "zod";
import {
  PACKAGE_VERSION,
  PUBLISH_LICENSE_FILE_NAMES,
  PUBLISH_WORKSPACE_DIRECTORY,
  RUNTIME_PACKAGE_DEFINITIONS,
  WRAPPER_PACKAGE_NAME,
  publishWorkspaceTextFiles,
} from "./package-manifests.ts";
import {
  SELECTED_NPM_DIST_TAG_SCHEMA,
  STAGING_NPM_DIST_TAG_SCHEMA,
  type NpmDistTag,
  type SelectedNpmDistTag,
  type StagingNpmDistTag,
} from "./npm-dist-tag.ts";
import {
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  type PublicPackageVersion,
} from "./package-version.ts";
import { PAYLOAD_CHECK_KIND } from "./payload-check.ts";
import {
  inspectReleaseSource,
  parseReleaseSourceExpectation,
  type ReleaseCommit,
  type ReleaseSourceExpectation,
} from "./release-source.ts";

const PNPM_PACK_RESULT_SCHEMA = z.object({
  filename: z.string().min(1),
  files: z.array(z.object({ path: z.string().min(1) })),
  name: z.string().min(1),
  version: z.string().min(1),
});
const NPM_PUBLICATION_CANDIDATE_MANIFEST_FILE = "npm-publication-candidate.json";
const NPM_PUBLICATION_CANDIDATE_SCHEMA = z
  .object({
    schemaVersion: z.literal(2),
    npmDistTag: SELECTED_NPM_DIST_TAG_SCHEMA,
    releaseCommit: z.string(),
    releaseTag: z.string(),
    releaseVersion: z.string(),
    packages: z.array(
      z
        .object({
          archiveFile: z.string().min(1),
          integrity: z.string().startsWith("sha512-"),
          packageName: z.string().min(1),
        })
        .strict(),
    ),
  })
  .strict();
const NPM_VIEW_INTEGRITY_SCHEMA = z.string().startsWith("sha512-");
const REGISTRY_VISIBILITY_ATTEMPTS = 60;
const REGISTRY_VISIBILITY_INTERVAL_MILLISECONDS = 2_000;
const NPM_REGISTRY_URL = "https://registry.npmjs.org/";
const EXPECTED_PNPM_VERSION = "11.7.0";
const WORKSPACE_ROOT_DIRECTORY = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const NPM_NOT_FOUND_CODE = "E404";
const TRANSIENT_NPM_FAILURE_PATTERN =
  /(?:EAI_AGAIN|ECONNRESET|ETIMEDOUT|ERR_SOCKET_TIMEOUT|E429|E50[0234]|HTTP (?:408|429|5\d\d))/u;
const NPM_COMMAND_MAXIMUM_OUTPUT_BYTES = 1_048_576;
const NPM_READ_COMMAND_TIMEOUT_MILLISECONDS = 30_000;
const NPM_MUTATION_COMMAND_TIMEOUT_MILLISECONDS = 600_000;

export type NpmPackageName =
  | (typeof RUNTIME_PACKAGE_DEFINITIONS)[number]["packageName"]
  | typeof WRAPPER_PACKAGE_NAME;

type ArchiveExpectation =
  | {
      readonly kind: "files";
      readonly requiredPaths: readonly string[];
    }
  | {
      readonly kind: "posix_executable";
      readonly executablePath: string;
      readonly requiredPaths: readonly string[];
    };

/** One staged npm package that must be reconciled with the registry. */
export interface NpmPublicationRequest {
  readonly archiveExpectation: ArchiveExpectation;
  readonly expectedManifestText: string;
  readonly expectedPackedManifestText: string;
  readonly expectedPackedPaths: readonly string[];
  readonly packageDirectory: string;
  readonly packageName: NpmPackageName;
}

type RemotePackageIntegrity =
  | { readonly kind: "absent" }
  | { readonly kind: "present"; readonly integrity: string };

/** Current registry target for one npm distribution tag. */
export type RemoteNpmDistTag =
  | { readonly kind: "absent" }
  | { readonly kind: "present"; readonly version: PublicPackageVersion };

/** Result of reading one npm channel from the registry. */
export type RemoteNpmDistTagInspection =
  | { readonly kind: "ok"; readonly remote: RemoteNpmDistTag }
  | { readonly kind: "transient_failure"; readonly detail: string }
  | { readonly kind: "permanent_failure"; readonly detail: string };

/** Decision for reconciling an npm channel with the release version. */
export type NpmDistTagDecision =
  | { readonly kind: "keep" }
  | { readonly kind: "update" }
  | { readonly kind: "would_move_backward"; readonly currentVersion: PublicPackageVersion };

/** Result of inspecting one immutable package version on npm. */
export type RemotePackageIntegrityInspection =
  | { readonly kind: "ok"; readonly integrity: RemotePackageIntegrity }
  | { readonly kind: "transient_failure"; readonly detail: string }
  | { readonly kind: "permanent_failure"; readonly detail: string };

/** Decision for reconciling one immutable package version. */
export type NpmPackageIntegrityDecision =
  | { readonly kind: "publish" }
  | { readonly kind: "skip" }
  | {
      readonly kind: "integrity_mismatch";
      readonly localIntegrity: string;
      readonly remoteIntegrity: string;
    };

type NpmCommandResult =
  | { readonly kind: "ok"; readonly stdout: string }
  | { readonly kind: "failed"; readonly detail: string };

type PackResult =
  | {
      readonly kind: "ok";
      readonly archivePath: string;
      readonly integrity: string;
      readonly packageName: string;
      readonly version: string;
    }
  | { readonly kind: "failed"; readonly detail: string };
type SuccessfulPackResult = Extract<PackResult, { readonly kind: "ok" }>;
type NpmPublicationCandidateManifest = z.infer<
  typeof NPM_PUBLICATION_CANDIDATE_SCHEMA
>;

interface PackagedNpmPublication {
  readonly packed: SuccessfulPackResult;
  readonly request: NpmPublicationRequest;
}

interface PreparedNpmPublication extends PackagedNpmPublication {
  readonly decision: Exclude<NpmPackageIntegrityDecision, { readonly kind: "integrity_mismatch" }>;
}

type PublishArchiveResult =
  | { readonly kind: "ok" }
  | { readonly kind: "failed"; readonly detail: string };
type UpdateNpmDistTagResult = PublishArchiveResult;

export type PublishNpmPackagesResult =
  | { readonly kind: "ok" }
  | {
      readonly kind: "failed";
      readonly packageName: NpmPackageName;
      readonly detail: string;
    };

/** Registry and package-manager operations used by the npm publication state machine. */
export interface NpmPublicationOperations {
  readonly inspectRemoteDistTag: (
    packageName: NpmPackageName,
    npmDistTag: NpmDistTag,
  ) => RemoteNpmDistTagInspection;
  readonly inspectRemoteIntegrity: (
    packageName: NpmPackageName,
  ) => RemotePackageIntegrityInspection;
  readonly packPackage: (
    request: NpmPublicationRequest,
    temporaryDirectory: string,
  ) => PackResult;
  readonly publishArchive: (
    archivePath: string,
    expectedIntegrity: string,
    npmDistTag: NpmDistTag,
  ) => PublishArchiveResult;
  readonly removeRemoteDistTag: (
    packageName: NpmPackageName,
    npmDistTag: NpmDistTag,
  ) => UpdateNpmDistTagResult;
  readonly sleepBeforeRetry: () => Promise<void>;
  readonly updateRemoteDistTag: (
    packageName: NpmPackageName,
    npmDistTag: NpmDistTag,
  ) => UpdateNpmDistTagResult;
}

/** Returns all carrier packages followed by the user-facing wrapper package. */
export function npmPublicationRequests(args: {
  readonly workspaceRootDirectory: string;
}): readonly NpmPublicationRequest[] {
  const publishWorkspace = path.join(args.workspaceRootDirectory, PUBLISH_WORKSPACE_DIRECTORY);
  const generatedTextFiles = publishWorkspaceTextFiles();
  return [
    ...RUNTIME_PACKAGE_DEFINITIONS.map((definition) => {
      const requiredPaths = [
        "package.json",
        "README.md",
        definition.payloadPath,
        ...PUBLISH_LICENSE_FILE_NAMES,
      ];
      const expectedManifestText = generatedTextFile(
        generatedTextFiles,
        `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/package.json`,
      );
      return {
        archiveExpectation:
          definition.payloadCheckKind === PAYLOAD_CHECK_KIND.POSIX_EXECUTABLE
            ? {
                kind: "posix_executable" as const,
                executablePath: definition.payloadPath,
                requiredPaths,
              }
            : { kind: "files" as const, requiredPaths },
        expectedManifestText,
        expectedPackedManifestText: expectedManifestText,
        expectedPackedPaths: requiredPaths,
        packageDirectory: path.join(publishWorkspace, "packages", definition.slug),
        packageName: definition.packageName,
      };
    }),
    (() => {
      const expectedManifestText = generatedTextFile(
        generatedTextFiles,
        `${PUBLISH_WORKSPACE_DIRECTORY}/sprawl-guard/package.json`,
      );
      return {
      archiveExpectation: {
        kind: "files" as const,
        requiredPaths: [
          "package.json",
          "README.md",
          "dist/bin/sprawl-guard.js",
          ...PUBLISH_LICENSE_FILE_NAMES,
        ],
      },
      expectedManifestText,
      expectedPackedManifestText: pnpmPackedWrapperManifest(expectedManifestText),
      expectedPackedPaths: wrapperPackedPaths(),
      packageDirectory: path.join(publishWorkspace, "sprawl-guard"),
      packageName: WRAPPER_PACKAGE_NAME,
      };
    })(),
  ];
}

/** Models pnpm's audited removal of its development-only selector from published metadata. */
function pnpmPackedWrapperManifest(manifestText: string): string {
  const manifest = JSON.parse(manifestText) as Record<string, unknown>;
  delete manifest.packageManager;
  return `${JSON.stringify(manifest, null, 2)}\n`;
}

function generatedTextFile(
  files: readonly { readonly relativePath: string; readonly text: string }[],
  relativePath: string,
): string {
  const file = files.find((candidate) => candidate.relativePath === relativePath);
  if (file === undefined) {
    throw new Error(`missing generated package manifest: ${relativePath}`);
  }
  return file.text;
}

function wrapperPackedPaths(): readonly string[] {
  const librarySourceDirectory = path.join(
    WORKSPACE_ROOT_DIRECTORY,
    "sprawl-guard",
    "src",
    "lib",
  );
  const compiledLibraryPaths = fs
    .readdirSync(librarySourceDirectory, { withFileTypes: true })
    .filter(
      (entry) =>
        entry.isFile()
        && entry.name.endsWith(".ts")
        && !entry.name.endsWith(".test.ts"),
    )
    .map((entry) => `dist/lib/${entry.name.slice(0, -3)}.js`);
  return [
    "package.json",
    "README.md",
    ...PUBLISH_LICENSE_FILE_NAMES,
    "dist/bin/sprawl-guard.js",
    ...compiledLibraryPaths,
  ];
}

type LoadNpmPublicationCandidateResult =
  | {
      readonly kind: "ok";
      readonly packedByPackageName: ReadonlyMap<NpmPackageName, SuccessfulPackResult>;
    }
  | {
      readonly kind: "failed";
      readonly packageName: NpmPackageName;
      readonly detail: string;
    };

/** Packs and records the complete npm candidate before registry authentication. */
export function prepareNpmPublicationCandidate(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly outputDirectory: string;
  readonly workspaceRootDirectory: string;
}): PublishNpmPackagesResult {
  const pnpmVersion = runCommand("pnpm", ["--version"], args.workspaceRootDirectory);
  if (pnpmVersion.kind === "failed") {
    return {
      kind: "failed",
      packageName: WRAPPER_PACKAGE_NAME,
      detail: pnpmVersion.detail,
    };
  }
  const versionFailure = validatePnpmVersion(pnpmVersion.stdout.trim());
  if (versionFailure !== null) {
    return {
      kind: "failed",
      packageName: WRAPPER_PACKAGE_NAME,
      detail: versionFailure,
    };
  }
  try {
    fs.mkdirSync(args.outputDirectory, { recursive: false });
  } catch (cause) {
    return {
      kind: "failed",
      packageName: WRAPPER_PACKAGE_NAME,
      detail: `failed to create npm candidate directory: ${String(cause)}`,
    };
  }

  const requests = npmPublicationRequests({
    workspaceRootDirectory: args.workspaceRootDirectory,
  });
  const packages: NpmPublicationCandidateManifest["packages"] = [];
  for (const request of requests) {
    const packed = packPackage(request, args.outputDirectory);
    if (packed.kind === "failed") {
      return failedPublication(request, packed.detail);
    }
    if (packed.packageName !== request.packageName || packed.version !== PACKAGE_VERSION) {
      return failedPublication(
        request,
        `packed identity ${packed.packageName}@${packed.version} does not match ${request.packageName}@${PACKAGE_VERSION}`,
      );
    }
    packages.push({
      archiveFile: path.basename(packed.archivePath),
      integrity: packed.integrity,
      packageName: request.packageName,
    });
  }

  const manifest: NpmPublicationCandidateManifest = {
    schemaVersion: 2,
    npmDistTag: args.npmDistTag,
    releaseCommit: args.expectation.commit,
    releaseTag: args.expectation.tag,
    releaseVersion: PACKAGE_VERSION,
    packages,
  };
  try {
    fs.writeFileSync(
      path.join(args.outputDirectory, NPM_PUBLICATION_CANDIDATE_MANIFEST_FILE),
      `${JSON.stringify(manifest, null, 2)}\n`,
      { flag: "wx" },
    );
  } catch (cause) {
    return {
      kind: "failed",
      packageName: WRAPPER_PACKAGE_NAME,
      detail: `failed to write npm publication candidate: ${String(cause)}`,
    };
  }
  return { kind: "ok" };
}

function loadNpmPublicationCandidate(args: {
  readonly candidateDirectory: string;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly requests: readonly NpmPublicationRequest[];
}): LoadNpmPublicationCandidateResult {
  let parsedJson: unknown;
  const manifestPath = path.join(
    args.candidateDirectory,
    NPM_PUBLICATION_CANDIDATE_MANIFEST_FILE,
  );
  const manifestEntryFailure = validateRegularCandidateFile(manifestPath);
  if (manifestEntryFailure !== null) {
    return candidateFailure(manifestEntryFailure);
  }
  try {
    parsedJson = JSON.parse(
      fs.readFileSync(manifestPath, "utf8"),
    ) as unknown;
  } catch (cause) {
    return candidateFailure(`failed to read npm candidate manifest: ${String(cause)}`);
  }
  const parsed = NPM_PUBLICATION_CANDIDATE_SCHEMA.safeParse(parsedJson);
  if (!parsed.success) {
    return candidateFailure(`invalid npm candidate manifest: ${z.prettifyError(parsed.error)}`);
  }
  const manifest = parsed.data;
  if (
    manifest.releaseCommit !== args.expectation.commit
    || manifest.releaseTag !== args.expectation.tag
    || manifest.releaseVersion !== PACKAGE_VERSION
    || manifest.npmDistTag !== args.npmDistTag
  ) {
    return candidateFailure("npm candidate does not match the frozen release source");
  }
  if (manifest.packages.length !== args.requests.length) {
    return candidateFailure("npm candidate does not contain all eight packages");
  }

  const expectedFiles = new Set<string>([NPM_PUBLICATION_CANDIDATE_MANIFEST_FILE]);
  const packedByPackageName = new Map<NpmPackageName, SuccessfulPackResult>();
  for (const [index, request] of args.requests.entries()) {
    const entry = manifest.packages[index];
    if (entry === undefined || entry.packageName !== request.packageName) {
      return candidateRequestFailure(request, "npm candidate package order or identity is invalid");
    }
    if (path.basename(entry.archiveFile) !== entry.archiveFile) {
      return candidateRequestFailure(request, "npm candidate archive name is not a basename");
    }
    expectedFiles.add(entry.archiveFile);
    const archivePath = path.join(args.candidateDirectory, entry.archiveFile);
    const archiveEntryFailure = validateRegularCandidateFile(archivePath);
    if (archiveEntryFailure !== null) {
      return candidateRequestFailure(request, archiveEntryFailure);
    }
    const validated = validateCandidateArchive({ archivePath, request });
    if (validated.kind === "failed") {
      return candidateRequestFailure(request, validated.detail);
    }
    if (validated.integrity !== entry.integrity) {
      return candidateRequestFailure(request, "npm candidate archive hash differs from its manifest");
    }
    packedByPackageName.set(request.packageName, validated);
  }

  let actualFiles: string[];
  try {
    actualFiles = fs.readdirSync(args.candidateDirectory).toSorted();
  } catch (cause) {
    return candidateFailure(`failed to inspect npm candidate directory: ${String(cause)}`);
  }
  const candidatePathFailure = validatePackedPaths({
    actualPaths: actualFiles,
    expectedPaths: [...expectedFiles],
  });
  if (candidatePathFailure !== null) {
    return candidateFailure(`npm candidate directory is invalid: ${candidatePathFailure}`);
  }
  return { kind: "ok", packedByPackageName };
}

function candidateFailure(detail: string): LoadNpmPublicationCandidateResult {
  return { kind: "failed", packageName: WRAPPER_PACKAGE_NAME, detail };
}

function candidateRequestFailure(
  request: NpmPublicationRequest,
  detail: string,
): LoadNpmPublicationCandidateResult {
  return { kind: "failed", packageName: request.packageName, detail };
}

function validateRegularCandidateFile(filePath: string): string | null {
  try {
    const status = fs.lstatSync(filePath);
    return status.isFile() && status.nlink === 1
      ? null
      : `npm candidate entry is not an independent regular file: ${filePath}`;
  } catch (cause) {
    return `failed to inspect npm candidate entry ${filePath}: ${String(cause)}`;
  }
}

/** Decides whether an immutable npm package version is new, safely resumed, or conflicting. */
export function reconcileNpmPackageIntegrity(args: {
  readonly localIntegrity: string;
  readonly remoteIntegrity: RemotePackageIntegrity;
}): NpmPackageIntegrityDecision {
  if (args.remoteIntegrity.kind === "absent") {
    return { kind: "publish" };
  }
  if (args.remoteIntegrity.integrity === args.localIntegrity) {
    return { kind: "skip" };
  }
  return {
    kind: "integrity_mismatch",
    localIntegrity: args.localIntegrity,
    remoteIntegrity: args.remoteIntegrity.integrity,
  };
}

/** Decides whether a channel is current, may advance, or would move backward. */
export function reconcileNpmDistTag(args: {
  readonly releaseVersion: PublicPackageVersion;
  readonly remote: RemoteNpmDistTag;
}): NpmDistTagDecision {
  if (args.remote.kind === "absent") {
    return { kind: "update" };
  }
  if (args.remote.version === args.releaseVersion) {
    return { kind: "keep" };
  }
  return gt(args.remote.version, args.releaseVersion)
    ? {
        kind: "would_move_backward",
        currentVersion: args.remote.version,
      }
    : { kind: "update" };
}

/** Validates the exact generated manifest before pnpm can run package scripts. */
export function validateStagedPackageManifest(args: {
  readonly actualText: string;
  readonly expectedText: string;
}): string | null {
  return args.actualText === args.expectedText
    ? null
    : "staged package.json differs from the generated release manifest";
}

function validatePackedPackageManifest(args: {
  readonly actualText: string;
  readonly expectedText: string;
}): string | null {
  try {
    return JSON.stringify(JSON.parse(args.actualText))
      === JSON.stringify(JSON.parse(args.expectedText))
      ? null
      : "packed package.json differs from the generated release manifest";
  } catch (cause) {
    return `packed package.json is invalid: ${String(cause)}`;
  }
}

/** Validates that a packed archive contains exactly the expected paths. */
export function validatePackedPaths(args: {
  readonly actualPaths: readonly string[];
  readonly expectedPaths: readonly string[];
}): string | null {
  const actual = args.actualPaths.toSorted();
  const expected = args.expectedPaths.toSorted();
  return actual.length === expected.length
    && actual.every((actualPath, index) => actualPath === expected[index])
    ? null
    : `packed paths differ: expected ${expected.join(", ")}; received ${actual.join(", ")}`;
}

/** Runs the ordered, resumable npm publication state machine through supplied operations. */
export async function publishNpmPublicationRequests(args: {
  readonly npmDistTag: SelectedNpmDistTag;
  readonly stagingNpmDistTag: StagingNpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly requests: readonly NpmPublicationRequest[];
  readonly temporaryDirectory: string;
}): Promise<PublishNpmPackagesResult> {
  const packagedPublications: PackagedNpmPublication[] = [];
  for (const request of args.requests) {
    const packed = args.operations.packPackage(request, args.temporaryDirectory);
    if (packed.kind === "failed") {
      return failedPublication(request, packed.detail);
    }
    if (packed.packageName !== request.packageName || packed.version !== PACKAGE_VERSION) {
      return failedPublication(
        request,
        `packed identity ${packed.packageName}@${packed.version} does not match ${request.packageName}@${PACKAGE_VERSION}`,
      );
    }
    packagedPublications.push({ packed, request });
  }

  const preparedPublications: PreparedNpmPublication[] = [];
  for (const publication of packagedPublications) {
    const { packed, request } = publication;
    const inspected = await inspectBeforePublication(request.packageName, args.operations);
    if (inspected.kind !== "ok") {
      return failedPublication(request, inspected.detail);
    }
    const decision = reconcileNpmPackageIntegrity({
      localIntegrity: packed.integrity,
      remoteIntegrity: inspected.integrity,
    });
    if (decision.kind === "integrity_mismatch") {
      return failedPublication(
        request,
        `registry integrity ${decision.remoteIntegrity} differs from staged integrity ${decision.localIntegrity}`,
      );
    }
    const inspectedDistTag = await inspectDistTagBeforePublication(
      request.packageName,
      args.npmDistTag,
      args.operations,
    );
    if (inspectedDistTag.kind !== "ok") {
      return failedPublication(request, inspectedDistTag.detail);
    }
    const distTagDecision = reconcileNpmDistTag({
      releaseVersion: PACKAGE_VERSION,
      remote: inspectedDistTag.remote,
    });
    if (distTagDecision.kind === "would_move_backward") {
      return failedPublication(
        request,
        `npm dist-tag ${args.npmDistTag} already targets newer version ${distTagDecision.currentVersion}`,
      );
    }
    preparedPublications.push({ ...publication, decision });
  }

  for (const { decision, packed, request } of preparedPublications) {
    if (decision.kind === "skip") {
      console.log(
        `already published with matching integrity: ${request.packageName}@${PACKAGE_VERSION}`,
      );
    } else {
      const reinspection = await inspectBeforePublication(
        request.packageName,
        args.operations,
      );
      if (reinspection.kind !== "ok") {
        return failedPublication(request, reinspection.detail);
      }
      const currentDecision = reconcileNpmPackageIntegrity({
        localIntegrity: packed.integrity,
        remoteIntegrity: reinspection.integrity,
      });
      if (currentDecision.kind === "integrity_mismatch") {
        return failedPublication(
          request,
          `registry integrity ${currentDecision.remoteIntegrity} differs from staged integrity ${currentDecision.localIntegrity}`,
        );
      }
      if (currentDecision.kind === "skip") {
        console.log(
          `already published with matching integrity: ${request.packageName}@${PACKAGE_VERSION}`,
        );
      } else {
        const published = args.operations.publishArchive(
          packed.archivePath,
          packed.integrity,
          args.stagingNpmDistTag,
        );
        if (published.kind === "failed") {
          return failedPublication(request, published.detail);
        }
        const visible = await waitForPublishedPackage(
          request.packageName,
          packed.integrity,
          args.operations,
        );
        if (visible !== null) {
          return failedPublication(request, visible);
        }
        console.log(`published ${request.packageName}@${PACKAGE_VERSION}`);
      }
    }
  }

  for (const { request } of preparedPublications) {
    const distTagFailure = await reconcilePublishedDistTag({
      npmDistTag: args.npmDistTag,
      operations: args.operations,
      packageName: request.packageName,
    });
    if (distTagFailure !== null) {
      return failedPublication(request, distTagFailure);
    }
  }

  for (const { request } of preparedPublications) {
    const stagingTagFailure = await removePublishedStagingDistTag({
      operations: args.operations,
      packageName: request.packageName,
      selectedNpmDistTag: args.npmDistTag,
      stagingNpmDistTag: args.stagingNpmDistTag,
    });
    if (stagingTagFailure !== null) {
      return failedPublication(request, stagingTagFailure);
    }
  }
  return { kind: "ok" };
}

/** Publishes absent packages and safely skips byte-identical versions from a resumed release. */
export async function publishNpmPackages(args: {
  readonly candidateDirectory: string;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly workspaceRootDirectory: string;
}): Promise<PublishNpmPackagesResult> {
  const requests = npmPublicationRequests(args);
  const candidate = loadNpmPublicationCandidate({
    candidateDirectory: args.candidateDirectory,
    expectation: args.expectation,
    npmDistTag: args.npmDistTag,
    requests,
  });
  if (candidate.kind === "failed") {
    return candidate;
  }
  return publishNpmPublicationRequests({
    npmDistTag: args.npmDistTag,
    operations: productionOperations(candidate.packedByPackageName),
    requests,
    stagingNpmDistTag: npmStagingDistTag(args.expectation.commit),
    temporaryDirectory: args.candidateDirectory,
  });
}

/** Returns the release-scoped tag used while uploading a complete package set. */
export function npmStagingDistTag(releaseCommit: ReleaseCommit): StagingNpmDistTag {
  return STAGING_NPM_DIST_TAG_SCHEMA.parse(`sg-candidate-${releaseCommit}`);
}

/** Validates a downloaded npm candidate without registry credentials. */
export function verifyNpmPublicationCandidate(args: {
  readonly candidateDirectory: string;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly workspaceRootDirectory: string;
}): PublishNpmPackagesResult {
  const loaded = loadNpmPublicationCandidate({
    candidateDirectory: args.candidateDirectory,
    expectation: args.expectation,
    npmDistTag: args.npmDistTag,
    requests: npmPublicationRequests(args),
  });
  return loaded.kind === "ok" ? { kind: "ok" } : loaded;
}

/** Validates the audited pnpm version used to form immutable package archives. */
export function validatePnpmVersion(actualVersion: string): string | null {
  return actualVersion === EXPECTED_PNPM_VERSION
    ? null
    : `pnpm ${EXPECTED_PNPM_VERSION} is required; found ${actualVersion}`;
}

function failedPublication(
  request: NpmPublicationRequest,
  detail: string,
): PublishNpmPackagesResult {
  return {
    kind: "failed",
    packageName: request.packageName,
    detail,
  };
}

async function inspectBeforePublication(
  packageName: NpmPackageName,
  operations: NpmPublicationOperations,
): Promise<RemotePackageIntegrityInspection> {
  for (let attempt = 0; attempt < REGISTRY_VISIBILITY_ATTEMPTS; attempt += 1) {
    const inspected = operations.inspectRemoteIntegrity(packageName);
    if (inspected.kind !== "transient_failure") {
      return inspected;
    }
    if (attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS) {
      await operations.sleepBeforeRetry();
    }
  }
  return {
    kind: "permanent_failure",
    detail: `npm inspection for ${packageName}@${PACKAGE_VERSION} exceeded the retry budget`,
  };
}

async function inspectDistTagBeforePublication(
  packageName: NpmPackageName,
  npmDistTag: NpmDistTag,
  operations: NpmPublicationOperations,
): Promise<RemoteNpmDistTagInspection> {
  for (let attempt = 0; attempt < REGISTRY_VISIBILITY_ATTEMPTS; attempt += 1) {
    const inspected = operations.inspectRemoteDistTag(packageName, npmDistTag);
    if (inspected.kind !== "transient_failure") {
      return inspected;
    }
    if (attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS) {
      await operations.sleepBeforeRetry();
    }
  }
  return {
    kind: "permanent_failure",
    detail: `npm dist-tag inspection for ${packageName}@${npmDistTag} exceeded the retry budget`,
  };
}

async function reconcilePublishedDistTag(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly packageName: NpmPackageName;
}): Promise<string | null> {
  const inspected = await inspectDistTagBeforePublication(
    args.packageName,
    args.npmDistTag,
    args.operations,
  );
  if (inspected.kind !== "ok") {
    return inspected.detail;
  }
  const decision = reconcileNpmDistTag({
    releaseVersion: PACKAGE_VERSION,
    remote: inspected.remote,
  });
  if (decision.kind === "keep") {
    return null;
  }
  if (decision.kind === "would_move_backward") {
    return `refusing to move npm dist-tag ${args.npmDistTag} backward from ${decision.currentVersion}`;
  }
  const updated = args.operations.updateRemoteDistTag(args.packageName, args.npmDistTag);
  if (updated.kind === "failed") {
    return updated.detail;
  }
  return waitForPublishedDistTag(args);
}

async function waitForPublishedDistTag(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly packageName: NpmPackageName;
}): Promise<string | null> {
  for (let attempt = 0; attempt < REGISTRY_VISIBILITY_ATTEMPTS; attempt += 1) {
    const inspected = args.operations.inspectRemoteDistTag(
      args.packageName,
      args.npmDistTag,
    );
    if (inspected.kind === "permanent_failure") {
      return inspected.detail;
    }
    if (inspected.kind === "ok" && inspected.remote.kind === "present") {
      if (inspected.remote.version === PACKAGE_VERSION) {
        return null;
      }
      if (gt(inspected.remote.version, PACKAGE_VERSION)) {
        return `npm dist-tag ${args.npmDistTag} advanced to ${inspected.remote.version}`;
      }
    }
    if (attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS) {
      await args.operations.sleepBeforeRetry();
    }
  }
  return `npm did not serve ${args.packageName}@${args.npmDistTag} at ${PACKAGE_VERSION} before the visibility timeout`;
}

async function removePublishedStagingDistTag(args: {
  readonly operations: NpmPublicationOperations;
  readonly packageName: NpmPackageName;
  readonly selectedNpmDistTag: SelectedNpmDistTag;
  readonly stagingNpmDistTag: StagingNpmDistTag;
}): Promise<string | null> {
  const inspected = await inspectDistTagBeforePublication(
    args.packageName,
    args.stagingNpmDistTag,
    args.operations,
  );
  if (inspected.kind !== "ok") {
    return inspected.detail;
  }
  if (
    inspected.remote.kind === "present"
    && inspected.remote.version !== PACKAGE_VERSION
  ) {
    return `refusing to remove npm staging tag ${args.stagingNpmDistTag} from ${inspected.remote.version}`;
  }
  const selected = await inspectDistTagBeforePublication(
    args.packageName,
    args.selectedNpmDistTag,
    args.operations,
  );
  if (selected.kind !== "ok") {
    return selected.detail;
  }
  if (
    selected.remote.kind !== "present"
    || selected.remote.version !== PACKAGE_VERSION
  ) {
    return `refusing to remove npm staging tag ${args.stagingNpmDistTag} because selected npm channel ${args.selectedNpmDistTag} no longer targets ${PACKAGE_VERSION}`;
  }
  if (inspected.remote.kind === "absent") {
    return null;
  }
  const removed = args.operations.removeRemoteDistTag(
    args.packageName,
    args.stagingNpmDistTag,
  );
  return removed.kind === "ok" ? null : removed.detail;
}

async function waitForPublishedPackage(
  packageName: NpmPackageName,
  expectedIntegrity: string,
  operations: NpmPublicationOperations,
): Promise<string | null> {
  for (let attempt = 0; attempt < REGISTRY_VISIBILITY_ATTEMPTS; attempt += 1) {
    const inspected = operations.inspectRemoteIntegrity(packageName);
    if (inspected.kind === "permanent_failure") {
      return inspected.detail;
    }
    if (inspected.kind === "ok" && inspected.integrity.kind === "present") {
      return inspected.integrity.integrity === expectedIntegrity
        ? null
        : `published integrity ${inspected.integrity.integrity} differs from uploaded integrity ${expectedIntegrity}`;
    }
    if (attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS) {
      await operations.sleepBeforeRetry();
    }
  }
  return `npm did not serve ${packageName}@${PACKAGE_VERSION} before the visibility timeout`;
}

function packPackage(request: NpmPublicationRequest, temporaryDirectory: string): PackResult {
  let manifestText: string;
  try {
    manifestText = fs.readFileSync(path.join(request.packageDirectory, "package.json"), "utf8");
  } catch (cause) {
    return { kind: "failed", detail: `failed to read staged package.json: ${String(cause)}` };
  }
  const manifestFailure = validateStagedPackageManifest({
    actualText: manifestText,
    expectedText: request.expectedManifestText,
  });
  if (manifestFailure !== null) {
    return { kind: "failed", detail: manifestFailure };
  }
  const packed = runCommand(
    "pnpm",
    ["pack", "--json", "--pack-destination", temporaryDirectory],
    request.packageDirectory,
  );
  if (packed.kind === "failed") {
    return packed;
  }
  const parsed = PNPM_PACK_RESULT_SCHEMA.safeParse(parseJson(packed.stdout));
  if (!parsed.success) {
    return {
      kind: "failed",
      detail: `pnpm pack returned invalid JSON: ${z.prettifyError(parsed.error)}`,
    };
  }
  const archivePath = path.resolve(parsed.data.filename);
  if (path.dirname(archivePath) !== path.resolve(temporaryDirectory)) {
    return {
      kind: "failed",
      detail: `pnpm pack wrote outside the temporary directory: ${archivePath}`,
    };
  }
  const packedPaths = new Set(parsed.data.files.map((file) => file.path));
  const exactPathsFailure = validatePackedPaths({
    actualPaths: [...packedPaths],
    expectedPaths: request.expectedPackedPaths,
  });
  if (exactPathsFailure !== null) {
    return { kind: "failed", detail: exactPathsFailure };
  }
  const missingPaths = request.archiveExpectation.requiredPaths.filter(
    (requiredPath) => !packedPaths.has(requiredPath),
  );
  if (missingPaths.length > 0) {
    return {
      kind: "failed",
      detail: `packed archive omitted required paths: ${missingPaths.join(", ")}`,
    };
  }
  if (request.archiveExpectation.kind === "posix_executable") {
    const modeVerification = verifyArchiveExecutableMode(
      archivePath,
      request.archiveExpectation.executablePath,
    );
    if (modeVerification !== null) {
      return { kind: "failed", detail: modeVerification };
    }
  }
  return {
    kind: "ok",
    archivePath,
    integrity: archiveIntegrity(archivePath),
    packageName: parsed.data.name,
    version: parsed.data.version,
  };
}

function validateCandidateArchive(args: {
  readonly archivePath: string;
  readonly request: NpmPublicationRequest;
}): PackResult {
  const listed = runCommand("tar", ["-tzf", args.archivePath]);
  if (listed.kind === "failed") {
    return listed;
  }
  const archivePaths = listed.stdout
    .split("\n")
    .filter((entry) => entry.length > 0);
  if (archivePaths.some((entry) => !entry.startsWith("package/"))) {
    return { kind: "failed", detail: "npm candidate contains a path outside package/" };
  }
  const packagePaths = archivePaths.map((entry) => entry.slice("package/".length));
  const pathFailure = validatePackedPaths({
    actualPaths: packagePaths,
    expectedPaths: args.request.expectedPackedPaths,
  });
  if (pathFailure !== null) {
    return { kind: "failed", detail: pathFailure };
  }

  const verbose = runCommand("tar", ["-tvzf", args.archivePath]);
  if (verbose.kind === "failed") {
    return verbose;
  }
  const verboseEntries = verbose.stdout.split("\n").filter((entry) => entry.length > 0);
  if (
    verboseEntries.length !== archivePaths.length
    || verboseEntries.some((entry) => !entry.startsWith("-"))
  ) {
    return { kind: "failed", detail: "npm candidate contains a non-regular archive entry" };
  }

  const manifest = runCommand("tar", [
    "-xOzf",
    args.archivePath,
    "package/package.json",
  ]);
  if (manifest.kind === "failed") {
    return manifest;
  }
  const manifestFailure = validatePackedPackageManifest({
    actualText: manifest.stdout,
    expectedText: args.request.expectedPackedManifestText,
  });
  if (manifestFailure !== null) {
    return { kind: "failed", detail: manifestFailure };
  }
  if (args.request.archiveExpectation.kind === "posix_executable") {
    const modeFailure = verifyArchiveExecutableMode(
      args.archivePath,
      args.request.archiveExpectation.executablePath,
    );
    if (modeFailure !== null) {
      return { kind: "failed", detail: modeFailure };
    }
  }
  return {
    kind: "ok",
    archivePath: args.archivePath,
    integrity: archiveIntegrity(args.archivePath),
    packageName: args.request.packageName,
    version: PACKAGE_VERSION,
  };
}

function verifyArchiveExecutableMode(
  archivePath: string,
  executablePath: string,
): string | null {
  const listed = runCommand("tar", ["-tvzf", archivePath, `package/${executablePath}`]);
  if (listed.kind === "failed") {
    return listed.detail;
  }
  const mode = listed.stdout.slice(0, 10);
  return /^-..x..x..x$/u.test(mode)
    ? null
    : `${executablePath} is not executable in ${archivePath}`;
}

function inspectRemoteIntegrity(packageName: NpmPackageName): RemotePackageIntegrityInspection {
  const viewed = runCommand("npm", [
    "view",
    `${packageName}@${PACKAGE_VERSION}`,
    "dist.integrity",
    "--json",
    "--registry",
    NPM_REGISTRY_URL,
  ]);
  if (viewed.kind === "failed") {
    if (viewed.detail.includes(NPM_NOT_FOUND_CODE)) {
      return { kind: "ok", integrity: { kind: "absent" } };
    }
    return TRANSIENT_NPM_FAILURE_PATTERN.test(viewed.detail)
      ? { kind: "transient_failure", detail: viewed.detail }
      : { kind: "permanent_failure", detail: viewed.detail };
  }
  const parsed = NPM_VIEW_INTEGRITY_SCHEMA.safeParse(parseJson(viewed.stdout));
  return parsed.success
    ? {
        kind: "ok",
        integrity: { kind: "present", integrity: parsed.data },
      }
    : {
        kind: "permanent_failure",
        detail: `npm view returned invalid integrity: ${z.prettifyError(parsed.error)}`,
      };
}

function inspectRemoteDistTag(
  packageName: NpmPackageName,
  npmDistTag: NpmDistTag,
): RemoteNpmDistTagInspection {
  const viewed = runCommand("npm", [
    "view",
    `${packageName}@${npmDistTag}`,
    "version",
    "--json",
    "--registry",
    NPM_REGISTRY_URL,
  ]);
  if (viewed.kind === "failed") {
    if (viewed.detail.includes(NPM_NOT_FOUND_CODE)) {
      return { kind: "ok", remote: { kind: "absent" } };
    }
    return TRANSIENT_NPM_FAILURE_PATTERN.test(viewed.detail)
      ? { kind: "transient_failure", detail: viewed.detail }
      : { kind: "permanent_failure", detail: viewed.detail };
  }
  const parsed = PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(parseJson(viewed.stdout));
  return parsed.success
    ? {
        kind: "ok",
        remote: { kind: "present", version: parsed.data },
      }
    : {
        kind: "permanent_failure",
        detail: `npm view returned invalid dist-tag version: ${z.prettifyError(parsed.error)}`,
      };
}

function updateRemoteDistTag(
  packageName: NpmPackageName,
  npmDistTag: NpmDistTag,
): UpdateNpmDistTagResult {
  const updated = runInteractiveNpmMutation([
    "dist-tag",
    "add",
    `${packageName}@${PACKAGE_VERSION}`,
    npmDistTag,
    "--registry",
    NPM_REGISTRY_URL,
  ]);
  return updated.kind === "ok"
    ? { kind: "ok" }
    : { kind: "failed", detail: updated.detail };
}

function removeRemoteDistTag(
  packageName: NpmPackageName,
  npmDistTag: NpmDistTag,
): UpdateNpmDistTagResult {
  return runInteractiveNpmMutation([
    "dist-tag",
    "rm",
    packageName,
    npmDistTag,
    "--registry",
    NPM_REGISTRY_URL,
  ]);
}

/** Returns the npm arguments for an interactive, local registry publication. */
export function npmPublishArguments(
  archivePath: string,
  npmDistTag: NpmDistTag,
): readonly string[] {
  return [
    "publish",
    archivePath,
    "--access",
    "public",
    "--ignore-scripts",
    "--tag",
    npmDistTag,
    "--registry",
    NPM_REGISTRY_URL,
  ];
}

/** Verifies that an archive still has the integrity recorded in its candidate manifest. */
export function validateArchiveIntegrity(
  archivePath: string,
  expectedIntegrity: string,
): string | null {
  try {
    const actualIntegrity = archiveIntegrity(archivePath);
    return actualIntegrity === expectedIntegrity
      ? null
      : `archive integrity changed: expected ${expectedIntegrity}; received ${actualIntegrity}`;
  } catch (cause) {
    return `archive integrity could not be verified: ${String(cause)}`;
  }
}

function archiveIntegrity(archivePath: string): string {
  return `sha512-${createHash("sha512").update(fs.readFileSync(archivePath)).digest("base64")}`;
}

function publishArchive(
  archivePath: string,
  expectedIntegrity: string,
  npmDistTag: NpmDistTag,
): PublishArchiveResult {
  const integrityFailure = validateArchiveIntegrity(archivePath, expectedIntegrity);
  if (integrityFailure !== null) {
    return { kind: "failed", detail: integrityFailure };
  }
  return runInteractiveNpmMutation(npmPublishArguments(archivePath, npmDistTag));
}

function runInteractiveNpmMutation(args: readonly string[]): PublishArchiveResult {
  const result = spawnSync("npm", args, {
    stdio: "inherit",
    timeout: NPM_MUTATION_COMMAND_TIMEOUT_MILLISECONDS,
  });
  return result.status === 0
    ? { kind: "ok" }
    : {
        kind: "failed",
        detail:
          result.error?.message
          ?? `npm ${args[0] ?? "command"} exited with status ${String(result.status)}`,
      };
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

function runCommand(
  command: "npm" | "pnpm" | "tar",
  args: readonly string[],
  cwd?: string,
): NpmCommandResult {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: NPM_COMMAND_MAXIMUM_OUTPUT_BYTES,
    stdio: ["ignore", "pipe", "pipe"],
    timeout: NPM_READ_COMMAND_TIMEOUT_MILLISECONDS,
  });
  if (result.status === 0) {
    return { kind: "ok", stdout: result.stdout };
  }
  return {
    kind: "failed",
    detail: [result.stdout, result.stderr, result.error?.message]
      .filter((value): value is string => value !== undefined && value.length > 0)
      .join("\n"),
  };
}

function productionOperations(
  packedByPackageName: ReadonlyMap<NpmPackageName, SuccessfulPackResult>,
): NpmPublicationOperations {
  return {
    inspectRemoteDistTag,
    inspectRemoteIntegrity,
    packPackage: (request) =>
      packedByPackageName.get(request.packageName) ?? {
        kind: "failed",
        detail: `npm candidate omitted ${request.packageName}`,
      },
    publishArchive,
    removeRemoteDistTag,
    sleepBeforeRetry: () =>
      new Promise<void>((resolve) =>
        setTimeout(resolve, REGISTRY_VISIBILITY_INTERVAL_MILLISECONDS),
      ),
    updateRemoteDistTag,
  };
}

async function main(): Promise<number> {
  const mode = process.argv[2];
  const candidateDirectoryArgument = process.argv[3];
  if (
    (mode !== "prepare" && mode !== "verify" && mode !== "publish")
    || candidateDirectoryArgument === undefined
  ) {
    console.error(
      "usage: publish-npm-packages.ts <prepare|verify|publish> <candidate-directory> <full-commit> <v-tag> <npm-dist-tag>",
    );
    return 1;
  }
  const parsedSource = parseReleaseSourceExpectation(process.argv.slice(4, 6), PACKAGE_VERSION);
  if (parsedSource.kind !== "ok") {
    console.error("release source requires a full commit and matching v-prefixed tag");
    return 1;
  }
  const parsedNpmDistTag = SELECTED_NPM_DIST_TAG_SCHEMA.safeParse(process.argv[6]);
  if (!parsedNpmDistTag.success || process.argv.length !== 7) {
    console.error("release source requires one validated npm dist-tag");
    return 1;
  }
  const candidateDirectory = path.resolve(candidateDirectoryArgument);
  const source = inspectReleaseSource({
    expectation: parsedSource.expectation,
    workspaceRoot: path.resolve(WORKSPACE_ROOT_DIRECTORY, ".."),
  });
  if (source.kind !== "ok") {
    console.error(`release source validation failed: ${JSON.stringify(source)}`);
    return 1;
  }
  const result =
    mode === "prepare"
      ? prepareNpmPublicationCandidate({
          expectation: parsedSource.expectation,
          npmDistTag: parsedNpmDistTag.data,
          outputDirectory: candidateDirectory,
          workspaceRootDirectory: WORKSPACE_ROOT_DIRECTORY,
        })
      : mode === "verify"
        ? verifyNpmPublicationCandidate({
            candidateDirectory,
            expectation: parsedSource.expectation,
            npmDistTag: parsedNpmDistTag.data,
            workspaceRootDirectory: WORKSPACE_ROOT_DIRECTORY,
          })
        : await publishNpmPackages({
          candidateDirectory,
          expectation: parsedSource.expectation,
          npmDistTag: parsedNpmDistTag.data,
          workspaceRootDirectory: WORKSPACE_ROOT_DIRECTORY,
        });
  if (result.kind === "ok") {
    return 0;
  }
  console.error(`failed to publish ${result.packageName}@${PACKAGE_VERSION}: ${result.detail}`);
  return 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(await main());
}
