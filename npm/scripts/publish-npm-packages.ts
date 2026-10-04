#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import {
  PACKAGE_VERSION,
  PUBLISH_LICENSE_FILE_NAMES,
  PUBLISH_WRAPPER_SCHEMAS,
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
import { PUBLIC_PACKAGE_VERSION_SCHEMA } from "./package-version.ts";
import { PAYLOAD_CHECK_KIND } from "./payload-check.ts";
import {
  reconcileNpmDistTag,
  type NpmPackageName,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
  type PackResult,
  type PublishArchiveResult,
  type PublishNpmPackagesResult,
  type RemoteNpmDistTagInspection,
  type RemotePackageIntegrityInspection,
  type SuccessfulPackResult,
  type ValidationResult,
} from "./npm-publication/model.ts";
import {
  inspectDistTagBeforePublication,
  prepareNpmPublications,
  publishPreparedNpmPublications,
  publishTrustedNpmPublicationRequests,
  waitForPublishedDistTag,
} from "./npm-publication/state.ts";
import {
  ABSOLUTE_DIRECTORY_PATH_SCHEMA,
  NPM_ARCHIVE_FILE_NAME_SCHEMA,
  NPM_ARCHIVE_PATH_SCHEMA,
  NPM_PACKAGE_INTEGRITY_SCHEMA,
  NPM_PACKAGE_NAME_SCHEMA,
  NPM_REGISTRY_URL,
  PACKAGE_RELATIVE_PATH_SCHEMA,
  parsePackageRelativePaths,
  type AbsoluteDirectoryPath,
  type NpmArchivePath,
  type NpmPackageIntegrity,
  type PackageRelativePath,
} from "./npm-publication/values.ts";
import {
  archiveIntegrity,
  validateArchiveIntegrity,
  validatePackedPackageManifest,
  validatePackedPaths,
  validatePnpmVersion,
  validateRegularCandidateFile,
  validateStagedPackageManifest,
} from "./npm-publication/validation.ts";
import { RELEASE_TAG_SCHEMA } from "./release-policy.ts";
import {
  inspectReleaseSource,
  parseReleaseSourceExpectation,
  RELEASE_COMMIT_SCHEMA,
  type ReleaseCommit,
  type ReleaseSourceExpectation,
} from "./release-source.ts";

const PNPM_PACK_RESULT_SCHEMA = z.object({
  filename: NPM_ARCHIVE_PATH_SCHEMA,
  files: z.array(z.object({ path: PACKAGE_RELATIVE_PATH_SCHEMA })),
  name: NPM_PACKAGE_NAME_SCHEMA,
  version: PUBLIC_PACKAGE_VERSION_SCHEMA,
});
const NPM_PUBLICATION_CANDIDATE_MANIFEST_FILE = "npm-publication-candidate.json";
const NPM_PUBLICATION_CANDIDATE_SCHEMA = z
  .object({
    schemaVersion: z.literal(2),
    npmDistTag: SELECTED_NPM_DIST_TAG_SCHEMA,
    releaseCommit: RELEASE_COMMIT_SCHEMA,
    releaseTag: RELEASE_TAG_SCHEMA,
    releaseVersion: PUBLIC_PACKAGE_VERSION_SCHEMA,
    packages: z.array(
      z
        .object({
          archiveFile: NPM_ARCHIVE_FILE_NAME_SCHEMA,
          integrity: NPM_PACKAGE_INTEGRITY_SCHEMA,
          packageName: NPM_PACKAGE_NAME_SCHEMA,
        })
        .strict(),
    ),
  })
  .strict();
const REGISTRY_VISIBILITY_INTERVAL_MILLISECONDS = 2_000;
const WORKSPACE_ROOT_DIRECTORY = ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(
  path.resolve(fileURLToPath(new URL("..", import.meta.url))),
);
const NPM_NOT_FOUND_CODE = "E404";
const TRANSIENT_NPM_FAILURE_PATTERN =
  /(?:EAI_AGAIN|ECONNRESET|ETIMEDOUT|ERR_SOCKET_TIMEOUT|E429|E50[0234]|HTTP (?:408|429|5\d\d))/u;
const NPM_COMMAND_MAXIMUM_OUTPUT_BYTES = 1_048_576;
const NPM_READ_COMMAND_TIMEOUT_MILLISECONDS = 30_000;
const NPM_MUTATION_COMMAND_TIMEOUT_MILLISECONDS = 600_000;

type NpmCommandResult =
  | { readonly kind: "ok"; readonly stdout: string }
  | { readonly kind: "failed"; readonly detail: string };

type NpmPublicationCandidateManifest = z.infer<
  typeof NPM_PUBLICATION_CANDIDATE_SCHEMA
>;
/** Returns all carrier packages followed by the user-facing wrapper package. */
export function npmPublicationRequests(args: {
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): readonly NpmPublicationRequest[] {
  const publishWorkspace = path.join(args.workspaceRootDirectory, PUBLISH_WORKSPACE_DIRECTORY);
  const generatedTextFiles = publishWorkspaceTextFiles();
  const wrapperManifestText = generatedTextFile(
    generatedTextFiles,
    `${PUBLISH_WORKSPACE_DIRECTORY}/sprawl-guard/package.json`,
  );
  return [
    ...RUNTIME_PACKAGE_DEFINITIONS.map((definition) => {
      const requiredPaths = parsePackageRelativePaths([
        "package.json",
        "README.md",
        definition.payloadPath,
        ...PUBLISH_LICENSE_FILE_NAMES,
      ]);
      const expectedManifestText = generatedTextFile(
        generatedTextFiles,
        `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/package.json`,
      );
      return {
        archiveExpectation:
          definition.payloadCheckKind === PAYLOAD_CHECK_KIND.POSIX_EXECUTABLE
            ? {
                kind: "posix_executable" as const,
                executablePath: PACKAGE_RELATIVE_PATH_SCHEMA.parse(
                  definition.payloadPath,
                ),
                requiredPaths,
              }
            : { kind: "files" as const, requiredPaths },
        expectedManifestText,
        expectedPackedManifestText: expectedManifestText,
        expectedPackedPaths: requiredPaths,
        packageDirectory: ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(
          path.join(publishWorkspace, "packages", definition.slug),
        ),
        packageName: definition.packageName,
      };
    }),
    {
      archiveExpectation: {
        kind: "files" as const,
        requiredPaths: WRAPPER_REQUIRED_PATHS,
      },
      expectedManifestText: wrapperManifestText,
      expectedPackedManifestText: pnpmPackedWrapperManifest(wrapperManifestText),
      expectedPackedPaths: wrapperPackedPaths(),
      packageDirectory: ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(
        path.join(publishWorkspace, "sprawl-guard"),
      ),
      packageName: WRAPPER_PACKAGE_NAME,
    },
  ];
}

/** Models pnpm's audited removal of its development-only selector from published metadata. */
function pnpmPackedWrapperManifest(
  manifestText: string,
): string {
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

function wrapperPackedPaths(): readonly PackageRelativePath[] {
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
  return parsePackageRelativePaths([
    ...WRAPPER_REQUIRED_PATHS,
    ...compiledLibraryPaths,
  ]);
}

const WRAPPER_REQUIRED_PATHS = parsePackageRelativePaths([
  "package.json",
  "README.md",
  ...PUBLISH_LICENSE_FILE_NAMES,
  "dist/bin/sprawl-guard.js",
  ...PUBLISH_WRAPPER_SCHEMAS.map((schema) => schema.packagePath),
]);

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

type LoadedNpmPublication =
  | {
      readonly kind: "ok";
      readonly operations: NpmPublicationOperations;
      readonly requests: readonly NpmPublicationRequest[];
    }
  | Extract<PublishNpmPackagesResult, { readonly kind: "failed" }>;

/** Packs and records the complete npm candidate before registry authentication. */
export function prepareNpmPublicationCandidate(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly outputDirectory: AbsoluteDirectoryPath;
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): PublishNpmPackagesResult {
  const pnpmVersion = runCommand("pnpm", ["--version"], args.workspaceRootDirectory);
  if (pnpmVersion.kind === "failed") {
    return {
      kind: "failed",
      packageName: WRAPPER_PACKAGE_NAME,
      detail: pnpmVersion.detail,
    };
  }
  const versionValidation = validatePnpmVersion(pnpmVersion.stdout.trim());
  if (versionValidation.kind === "invalid") {
    return {
      kind: "failed",
      packageName: WRAPPER_PACKAGE_NAME,
      detail: versionValidation.detail,
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
      archiveFile: NPM_ARCHIVE_FILE_NAME_SCHEMA.parse(
        path.basename(packed.archivePath),
      ),
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
  readonly candidateDirectory: AbsoluteDirectoryPath;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly requests: readonly NpmPublicationRequest[];
}): LoadNpmPublicationCandidateResult {
  let parsedJson: unknown;
  const manifestPath = path.join(
    args.candidateDirectory,
    NPM_PUBLICATION_CANDIDATE_MANIFEST_FILE,
  );
  const manifestEntryValidation = validateRegularCandidateFile(manifestPath);
  if (manifestEntryValidation.kind === "invalid") {
    return candidateFailure(manifestEntryValidation.detail);
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
    const archivePath = NPM_ARCHIVE_PATH_SCHEMA.parse(
      path.join(args.candidateDirectory, entry.archiveFile),
    );
    const archiveEntryValidation = validateRegularCandidateFile(archivePath);
    if (archiveEntryValidation.kind === "invalid") {
      return candidateRequestFailure(request, archiveEntryValidation.detail);
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
  const candidatePathValidation = validatePackedPaths({
    actualPaths: actualFiles,
    expectedPaths: [...expectedFiles],
  });
  if (candidatePathValidation.kind === "invalid") {
    return candidateFailure(
      `npm candidate directory is invalid: ${candidatePathValidation.detail}`,
    );
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

/** Runs the ordered, resumable npm publication state machine through supplied operations. */
export async function publishNpmPublicationRequests(args: {
  readonly npmDistTag: SelectedNpmDistTag;
  readonly stagingNpmDistTag: StagingNpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly requests: readonly NpmPublicationRequest[];
  readonly temporaryDirectory: AbsoluteDirectoryPath;
}): Promise<PublishNpmPackagesResult> {
  const prepared = await prepareNpmPublications(args);
  if (prepared.kind === "failed") {
    return prepared;
  }
  const preparedPublications = prepared.publications;

  const publication = await publishPreparedNpmPublications({
    operations: args.operations,
    publications: preparedPublications,
    route: {
      kind: "local_staging",
      publicationDistTag: args.stagingNpmDistTag,
    },
  });
  if (publication.kind === "failed") {
    return publication;
  }

  for (const { request } of preparedPublications) {
    const distTag = await reconcilePublishedDistTag({
      npmDistTag: args.npmDistTag,
      operations: args.operations,
      packageName: request.packageName,
    });
    if (distTag.kind === "failed") {
      return failedPublication(request, distTag.detail);
    }
  }

  for (const { request } of preparedPublications) {
    const stagingTag = await removePublishedStagingDistTag({
      operations: args.operations,
      packageName: request.packageName,
      selectedNpmDistTag: args.npmDistTag,
      stagingNpmDistTag: args.stagingNpmDistTag,
    });
    if (stagingTag.kind === "failed") {
      return failedPublication(request, stagingTag.detail);
    }
  }
  return { kind: "ok" };
}

/** Publishes absent packages and safely skips byte-identical versions from a resumed release. */
export async function publishNpmPackages(args: {
  readonly candidateDirectory: AbsoluteDirectoryPath;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): Promise<PublishNpmPackagesResult> {
  const loaded = loadNpmPublicationForRelease(args);
  if (loaded.kind === "failed") {
    return loaded;
  }
  return publishNpmPublicationRequests({
    npmDistTag: args.npmDistTag,
    operations: loaded.operations,
    requests: loaded.requests,
    stagingNpmDistTag: npmStagingDistTag(args.expectation.commit),
    temporaryDirectory: args.candidateDirectory,
  });
}

/** Publishes a validated candidate through npm's GitHub Actions trusted publisher. */
export async function publishTrustedNpmPackages(args: {
  readonly candidateDirectory: AbsoluteDirectoryPath;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): Promise<PublishNpmPackagesResult> {
  const loaded = loadNpmPublicationForRelease(args);
  if (loaded.kind === "failed") {
    return loaded;
  }
  return publishTrustedNpmPublicationRequests({
    npmDistTag: args.npmDistTag,
    operations: loaded.operations,
    requests: loaded.requests,
    temporaryDirectory: args.candidateDirectory,
  });
}

function loadNpmPublicationForRelease(args: {
  readonly candidateDirectory: AbsoluteDirectoryPath;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): LoadedNpmPublication {
  const requests = npmPublicationRequests(args);
  const candidate = loadNpmPublicationCandidate({
    candidateDirectory: args.candidateDirectory,
    expectation: args.expectation,
    npmDistTag: args.npmDistTag,
    requests,
  });
  return candidate.kind === "failed"
    ? candidate
    : {
        kind: "ok",
        operations: productionOperations(candidate.packedByPackageName),
        requests,
      };
}

/** Returns the release-scoped tag used while uploading a complete package set. */
export function npmStagingDistTag(releaseCommit: ReleaseCommit): StagingNpmDistTag {
  return STAGING_NPM_DIST_TAG_SCHEMA.parse(`sg-candidate-${releaseCommit}`);
}

/** Validates a downloaded npm candidate without registry credentials. */
export function verifyNpmPublicationCandidate(args: {
  readonly candidateDirectory: AbsoluteDirectoryPath;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: SelectedNpmDistTag;
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): PublishNpmPackagesResult {
  const loaded = loadNpmPublicationCandidate({
    candidateDirectory: args.candidateDirectory,
    expectation: args.expectation,
    npmDistTag: args.npmDistTag,
    requests: npmPublicationRequests(args),
  });
  return loaded.kind === "ok" ? { kind: "ok" } : loaded;
}

function failedPublication(
  request: NpmPublicationRequest,
  detail: string,
): Extract<PublishNpmPackagesResult, { readonly kind: "failed" }> {
  return {
    kind: "failed",
    packageName: request.packageName,
    detail,
  };
}

async function reconcilePublishedDistTag(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly packageName: NpmPackageName;
}): Promise<PublishArchiveResult> {
  const inspected = await inspectDistTagBeforePublication(
    args.packageName,
    args.npmDistTag,
    args.operations,
  );
  if (inspected.kind !== "ok") {
    return { kind: "failed", detail: inspected.detail };
  }
  const decision = reconcileNpmDistTag({
    releaseVersion: PACKAGE_VERSION,
    remote: inspected.remote,
  });
  if (decision.kind === "keep") {
    return { kind: "ok" };
  }
  if (decision.kind === "would_move_backward") {
    return {
      kind: "failed",
      detail: `refusing to move npm dist-tag ${args.npmDistTag} backward from ${decision.currentVersion}`,
    };
  }
  const updated = args.operations.updateRemoteDistTag(args.packageName, args.npmDistTag);
  if (updated.kind === "failed") {
    return updated;
  }
  return waitForPublishedDistTag(args);
}

async function removePublishedStagingDistTag(args: {
  readonly operations: NpmPublicationOperations;
  readonly packageName: NpmPackageName;
  readonly selectedNpmDistTag: SelectedNpmDistTag;
  readonly stagingNpmDistTag: StagingNpmDistTag;
}): Promise<PublishArchiveResult> {
  const inspected = await inspectDistTagBeforePublication(
    args.packageName,
    args.stagingNpmDistTag,
    args.operations,
  );
  if (inspected.kind !== "ok") {
    return { kind: "failed", detail: inspected.detail };
  }
  if (
    inspected.remote.kind === "present"
    && inspected.remote.version !== PACKAGE_VERSION
  ) {
    return {
      kind: "failed",
      detail: `refusing to remove npm staging tag ${args.stagingNpmDistTag} from ${inspected.remote.version}`,
    };
  }
  const selected = await inspectDistTagBeforePublication(
    args.packageName,
    args.selectedNpmDistTag,
    args.operations,
  );
  if (selected.kind !== "ok") {
    return { kind: "failed", detail: selected.detail };
  }
  if (
    selected.remote.kind !== "present"
    || selected.remote.version !== PACKAGE_VERSION
  ) {
    return {
      kind: "failed",
      detail: `refusing to remove npm staging tag ${args.stagingNpmDistTag} because selected npm channel ${args.selectedNpmDistTag} no longer targets ${PACKAGE_VERSION}`,
    };
  }
  if (inspected.remote.kind === "absent") {
    return { kind: "ok" };
  }
  const removed = args.operations.removeRemoteDistTag(
    args.packageName,
    args.stagingNpmDistTag,
  );
  return removed;
}

function packPackage(
  request: NpmPublicationRequest,
  temporaryDirectory: AbsoluteDirectoryPath,
): PackResult {
  let manifestText: string;
  try {
    manifestText = fs.readFileSync(path.join(request.packageDirectory, "package.json"), "utf8");
  } catch (cause) {
    return { kind: "failed", detail: `failed to read staged package.json: ${String(cause)}` };
  }
  const manifestValidation = validateStagedPackageManifest({
    actualText: manifestText,
    expectedText: request.expectedManifestText,
  });
  if (manifestValidation.kind === "invalid") {
    return { kind: "failed", detail: manifestValidation.detail };
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
  const archivePath = parsed.data.filename;
  if (path.dirname(archivePath) !== path.resolve(temporaryDirectory)) {
    return {
      kind: "failed",
      detail: `pnpm pack wrote outside the temporary directory: ${archivePath}`,
    };
  }
  const packedPaths = new Set(parsed.data.files.map((file) => file.path));
  const exactPathsValidation = validatePackedPaths({
    actualPaths: [...packedPaths],
    expectedPaths: request.expectedPackedPaths,
  });
  if (exactPathsValidation.kind === "invalid") {
    return { kind: "failed", detail: exactPathsValidation.detail };
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
    const modeValidation = verifyArchiveExecutableMode(
      archivePath,
      request.archiveExpectation.executablePath,
    );
    if (modeValidation.kind === "invalid") {
      return { kind: "failed", detail: modeValidation.detail };
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
  readonly archivePath: NpmArchivePath;
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
  const pathValidation = validatePackedPaths({
    actualPaths: packagePaths,
    expectedPaths: args.request.expectedPackedPaths,
  });
  if (pathValidation.kind === "invalid") {
    return { kind: "failed", detail: pathValidation.detail };
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
  const manifestValidation = validatePackedPackageManifest({
    actualText: manifest.stdout,
    expectedText: args.request.expectedPackedManifestText,
  });
  if (manifestValidation.kind === "invalid") {
    return { kind: "failed", detail: manifestValidation.detail };
  }
  if (args.request.archiveExpectation.kind === "posix_executable") {
    const modeValidation = verifyArchiveExecutableMode(
      args.archivePath,
      args.request.archiveExpectation.executablePath,
    );
    if (modeValidation.kind === "invalid") {
      return { kind: "failed", detail: modeValidation.detail };
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
  archivePath: NpmArchivePath,
  executablePath: PackageRelativePath,
): ValidationResult {
  const listed = runCommand("tar", ["-tvzf", archivePath, `package/${executablePath}`]);
  if (listed.kind === "failed") {
    return { kind: "invalid", detail: listed.detail };
  }
  const mode = listed.stdout.slice(0, 10);
  return /^-..x..x..x$/u.test(mode)
    ? { kind: "valid" }
    : {
        kind: "invalid",
        detail: `${executablePath} is not executable in ${archivePath}`,
      };
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
  const parsed = NPM_PACKAGE_INTEGRITY_SCHEMA.safeParse(parseJson(viewed.stdout));
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
): PublishArchiveResult {
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
): PublishArchiveResult {
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
  archivePath: NpmArchivePath,
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

function publishArchive(
  archivePath: NpmArchivePath,
  expectedIntegrity: NpmPackageIntegrity,
  npmDistTag: NpmDistTag,
): PublishArchiveResult {
  const integrityValidation = validateArchiveIntegrity(archivePath, expectedIntegrity);
  if (integrityValidation.kind === "invalid") {
    return { kind: "failed", detail: integrityValidation.detail };
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
  cwd?: AbsoluteDirectoryPath,
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
    (
      mode !== "prepare"
      && mode !== "verify"
      && mode !== "publish"
      && mode !== "publish-trusted"
    )
    || candidateDirectoryArgument === undefined
  ) {
    console.error(
      "usage: publish-npm-packages.ts <prepare|verify|publish|publish-trusted> <candidate-directory> <full-commit> <v-tag> <npm-dist-tag>",
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
  const candidateDirectory = ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(
    path.resolve(candidateDirectoryArgument),
  );
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
        : await (mode === "publish" ? publishNpmPackages : publishTrustedNpmPackages)({
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
