#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import {
  PACKAGE_VERSION,
  PUBLISH_LICENSE_FILE_NAMES,
  PUBLISH_WRAPPER_BUILD_FILES,
  PUBLISH_WRAPPER_DOCUMENTS,
  PUBLISH_WORKSPACE_DIRECTORY,
  RUNTIME_PACKAGE_DEFINITIONS,
  WRAPPER_PACKAGE_NAME,
  publishWorkspaceTextFiles,
} from "./package-manifests.ts";
import { NPM_DIST_TAG_SCHEMA, type NpmDistTag } from "./npm-dist-tag.ts";
import { PUBLIC_PACKAGE_VERSION_SCHEMA } from "./package-version.ts";
import { PAYLOAD_CHECK_KIND } from "./payload-check.ts";
import {
  type NpmPackageName,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
  type NpmReleasePackages,
  type PackResult,
  type PublishArchiveResult,
  type PublishNpmPackagesResult,
  type RemoteNpmDistTagInspection,
  type RemotePackageIntegrityInspection,
  type SuccessfulPackResult,
  type ValidationResult,
  releasePackageList,
} from "./npm-publication/model.ts";
import {
  packPublication,
  publishNpmPublicationRequests,
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
    npmDistTag: NPM_DIST_TAG_SCHEMA,
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
const NPM_PUBLISH_COMMAND_TIMEOUT_MILLISECONDS = 600_000;

type NpmCommandResult =
  | { readonly kind: "ok"; readonly stdout: string }
  | { readonly kind: "failed"; readonly detail: string };

type NpmPublicationCandidateManifest = z.infer<
  typeof NPM_PUBLICATION_CANDIDATE_SCHEMA
>;
/** Returns the platform packages and the user-facing wrapper package that depends on them. */
export function npmPublicationRequests(args: {
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): NpmReleasePackages<NpmPublicationRequest> {
  const publishWorkspace = path.join(args.workspaceRootDirectory, PUBLISH_WORKSPACE_DIRECTORY);
  const generatedTextFiles = publishWorkspaceTextFiles();
  const wrapperManifestText = generatedTextFile(
    generatedTextFiles,
    `${PUBLISH_WORKSPACE_DIRECTORY}/sprawl-guard/package.json`,
  );
  return {
    platformPackages: RUNTIME_PACKAGE_DEFINITIONS.map((definition) => {
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
    wrapper: {
      archiveExpectation: {
        kind: "files" as const,
        requiredPaths: WRAPPER_REQUIRED_PATHS,
      },
      expectedManifestText: wrapperManifestText,
      expectedPackedManifestText: pnpmPackedWrapperManifest(wrapperManifestText),
      expectedPackedPaths: WRAPPER_REQUIRED_PATHS,
      packageDirectory: ABSOLUTE_DIRECTORY_PATH_SCHEMA.parse(
        path.join(publishWorkspace, "sprawl-guard"),
      ),
      packageName: WRAPPER_PACKAGE_NAME,
    },
  };
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

const WRAPPER_REQUIRED_PATHS = parsePackageRelativePaths([
  "package.json",
  "README.md",
  ...PUBLISH_LICENSE_FILE_NAMES,
  ...PUBLISH_WRAPPER_BUILD_FILES,
  ...PUBLISH_WRAPPER_DOCUMENTS.map((document) => document.packagePath),
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

/** Packs and records the complete npm candidate before registry authentication. */
export function prepareNpmPublicationCandidate(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: NpmDistTag;
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

  const requests = releasePackageList(npmPublicationRequests({
    workspaceRootDirectory: args.workspaceRootDirectory,
  }));
  const packages: NpmPublicationCandidateManifest["packages"] = [];
  for (const request of requests) {
    const packaged = packPublication({
      packPackage,
      request,
      temporaryDirectory: args.outputDirectory,
    });
    if (packaged.kind === "failed") {
      return packaged;
    }
    const { packed } = packaged.value;
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
  readonly npmDistTag: NpmDistTag;
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

/**
 * Publishes a validated candidate on the selected channel, skipping byte-identical versions from
 * a resumed release. Authentication comes from npm trusted publishing in CI or an interactive
 * login locally.
 */
export async function publishNpmPackages(args: {
  readonly candidateDirectory: AbsoluteDirectoryPath;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: NpmDistTag;
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): Promise<PublishNpmPackagesResult> {
  const requests = npmPublicationRequests(args);
  const candidate = loadNpmPublicationCandidate({
    candidateDirectory: args.candidateDirectory,
    expectation: args.expectation,
    npmDistTag: args.npmDistTag,
    requests: releasePackageList(requests),
  });
  if (candidate.kind === "failed") {
    return candidate;
  }
  return publishNpmPublicationRequests({
    npmDistTag: args.npmDistTag,
    operations: productionOperations(candidate.packedByPackageName),
    requests,
    temporaryDirectory: args.candidateDirectory,
  });
}

/** Validates a downloaded npm candidate without registry credentials. */
export function verifyNpmPublicationCandidate(args: {
  readonly candidateDirectory: AbsoluteDirectoryPath;
  readonly expectation: ReleaseSourceExpectation;
  readonly npmDistTag: NpmDistTag;
  readonly workspaceRootDirectory: AbsoluteDirectoryPath;
}): PublishNpmPackagesResult {
  const loaded = loadNpmPublicationCandidate({
    candidateDirectory: args.candidateDirectory,
    expectation: args.expectation,
    npmDistTag: args.npmDistTag,
    requests: releasePackageList(npmPublicationRequests(args)),
  });
  return loaded.kind === "ok" ? { kind: "ok" } : loaded;
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

/** Returns the npm arguments that publish one archive directly on the selected channel. */
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
  // Inherit the terminal so a local interactive login can show npm's browser approval prompt.
  const result = spawnSync("npm", npmPublishArguments(archivePath, npmDistTag), {
    stdio: "inherit",
    timeout: NPM_PUBLISH_COMMAND_TIMEOUT_MILLISECONDS,
  });
  return result.status === 0
    ? { kind: "ok" }
    : {
        kind: "failed",
        detail:
          result.error?.message
          ?? `npm publish exited with status ${String(result.status)}`,
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
    sleepBeforeRetry: () =>
      new Promise<void>((resolve) =>
        setTimeout(resolve, REGISTRY_VISIBILITY_INTERVAL_MILLISECONDS),
      ),
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
  const parsedNpmDistTag = NPM_DIST_TAG_SCHEMA.safeParse(process.argv[6]);
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
