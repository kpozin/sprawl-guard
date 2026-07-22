#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  parse as parseToml,
  patch as patchToml,
} from "@decimalturn/toml-patch";
import { z } from "zod";
import {
  loadPublicPackageVersion,
  PublicPackageVersionFileError,
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  type PublicPackageVersion,
} from "./package-version.ts";

const CARGO_METADATA_SCHEMA = z.object({
  packages: z.array(
    z.object({
      dependencies: z.array(
        z.object({
          name: z.string(),
          req: z.string(),
        }),
      ),
      name: z.string(),
      version: PUBLIC_PACKAGE_VERSION_SCHEMA,
    }),
  ),
});
const CLI_PACKAGE_NAME = "sprawl-guard" as const;
const LIBRARY_PACKAGE_NAME = "sprawl-guard-lib" as const;
const CLI_MANIFEST_SCHEMA = z
  .object({
    package: z
      .object({
        name: z.literal(CLI_PACKAGE_NAME),
        version: PUBLIC_PACKAGE_VERSION_SCHEMA,
      })
      .passthrough(),
    dependencies: z
      .object({
        [LIBRARY_PACKAGE_NAME]: z.object({ version: z.string() }).passthrough(),
      })
      .passthrough(),
  })
  .passthrough();
const LIBRARY_MANIFEST_SCHEMA = z
  .object({
    package: z
      .object({
        name: z.literal(LIBRARY_PACKAGE_NAME),
        version: PUBLIC_PACKAGE_VERSION_SCHEMA,
      })
      .passthrough(),
  })
  .passthrough();

type PackageName = typeof CLI_PACKAGE_NAME | typeof LIBRARY_PACKAGE_NAME;

interface CurrentVersions {
  readonly cargoVersion: PublicPackageVersion;
  readonly npmVersion: PublicPackageVersion;
}

/** Result of synchronizing all checked-in public package version sources. */
export type SetPublicPackageVersionResult =
  | { readonly kind: "ok" }
  | {
      readonly kind: "invalid_version";
      readonly detail: string;
      readonly value: string;
    }
  | {
      readonly kind: "version_mismatch";
      readonly cargoVersion: PublicPackageVersion;
      readonly npmVersion: PublicPackageVersion;
    }
  | { readonly kind: "invalid_workspace"; readonly detail: string }
  | {
      readonly kind: "cleanup_failed";
      readonly detail: string;
      readonly retainedPaths: readonly string[];
    }
  | {
      readonly kind: "write_failed";
      readonly detail: string;
      readonly retainedPaths: readonly string[];
    };

/** Result of checking that every public package version source agrees. */
export type PublicPackageVersionConsistencyResult =
  | { readonly kind: "ok"; readonly version: PublicPackageVersion }
  | Extract<
      SetPublicPackageVersionResult,
      { readonly kind: "version_mismatch" | "invalid_workspace" }
    >;

/** Repository layout containing every authoritative public package version source. */
export interface PublicPackageVersionLayout {
  readonly kind: "nested" | "public";
  readonly workspaceRoot: string;
  readonly cliManifest: string;
  readonly libraryManifest: string;
  readonly npmVersion: string;
}

/** Result of resolving version-source paths from the updater script location. */
export type ResolvePublicPackageVersionLayoutResult =
  | { readonly kind: "ok"; readonly layout: PublicPackageVersionLayout }
  | {
      readonly kind: "invalid_layout";
      readonly candidates: readonly {
        readonly kind: PublicPackageVersionLayout["kind"];
        readonly missingPaths: readonly string[];
        readonly workspaceRoot: string;
      }[];
      readonly reason: "ambiguous_match" | "no_match";
      readonly scriptPath: string;
    };

/** Updates Cargo and npm version sources as one validated operation. */
export function setPublicPackageVersion(args: {
  readonly layout: PublicPackageVersionLayout;
  readonly newVersion: string;
}): SetPublicPackageVersionResult {
  const parsedVersion = PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(
    args.newVersion,
  );
  if (!parsedVersion.success) {
    return {
      kind: "invalid_version",
      detail: parsedVersion.error.issues
        .map(({ message }) => message)
        .join("; "),
      value: args.newVersion,
    };
  }
  const checked = checkPublicPackageVersionConsistency(args.layout);
  if (checked.kind !== "ok") {
    return checked;
  }

  const originalFiles = readVersionFiles(args.layout);
  if (originalFiles.kind !== "ok") {
    return originalFiles;
  }
  const rewritten = rewriteVersionFiles({
    currentVersion: checked.version,
    newVersion: parsedVersion.data,
    originalFiles: originalFiles.files,
  });
  if (rewritten.kind !== "ok") {
    return rewritten;
  }
  const written = writeVersionFiles(args.layout, rewritten.files);
  if (written.kind === "write_failed") {
    return written;
  }

  const verified = loadCurrentVersions(args.layout);
  if (
    verified.kind === "ok" &&
    verified.versions.cargoVersion === parsedVersion.data &&
    verified.versions.npmVersion === parsedVersion.data
  ) {
    return written;
  }

  const verificationFailure =
    verified.kind === "ok"
      ? {
          kind: "invalid_workspace" as const,
          detail:
            "updated package versions did not match the requested version",
        }
      : verified;
  const restored = writeVersionFiles(args.layout, originalFiles.files);
  if (restored.kind === "write_failed") {
    return {
      kind: "write_failed",
      detail: [
        `version verification failed: ${verificationFailure.detail}`,
        ...(written.kind === "cleanup_failed"
          ? [`initial cleanup failed: ${written.detail}`]
          : []),
        `restoration failed: ${restored.detail}`,
      ].join("; "),
      retainedPaths: uniquePaths([
        ...(written.kind === "cleanup_failed" ? written.retainedPaths : []),
        ...restored.retainedPaths,
      ]),
    };
  }
  const cleanupFailures = [
    ...(written.kind === "cleanup_failed"
      ? [`initial cleanup failed: ${written.detail}`]
      : []),
    ...(restored.kind === "cleanup_failed"
      ? [`restoration cleanup failed: ${restored.detail}`]
      : []),
  ];
  if (cleanupFailures.length > 0) {
    return {
      kind: "cleanup_failed",
      detail: [
        `version verification failed: ${verificationFailure.detail}`,
        ...cleanupFailures,
      ].join("; "),
      retainedPaths: uniquePaths([
        ...(written.kind === "cleanup_failed" ? written.retainedPaths : []),
        ...(restored.kind === "cleanup_failed" ? restored.retainedPaths : []),
      ]),
    };
  }
  return verificationFailure;
}

/** Checks Cargo and npm package versions without modifying the workspace. */
export function checkPublicPackageVersionConsistency(
  layout: PublicPackageVersionLayout,
): PublicPackageVersionConsistencyResult {
  const loaded = loadCurrentVersions(layout);
  if (loaded.kind !== "ok") {
    return loaded;
  }
  if (loaded.versions.cargoVersion !== loaded.versions.npmVersion) {
    return {
      kind: "version_mismatch",
      cargoVersion: loaded.versions.cargoVersion,
      npmVersion: loaded.versions.npmVersion,
    };
  }
  return { kind: "ok", version: loaded.versions.cargoVersion };
}

/** Resolves and validates either the nested or exported public layout. */
export function resolvePublicPackageVersionLayout(
  scriptPath: string,
): ResolvePublicPackageVersionLayoutResult {
  const npmWorkspaceRoot = path.dirname(path.dirname(scriptPath));
  const candidates: readonly PublicPackageVersionLayout[] = [
    {
      kind: "nested",
      workspaceRoot: path.resolve(npmWorkspaceRoot, "../../.."),
      cliManifest: path.resolve(
        npmWorkspaceRoot,
        "../../sprawl-guard/Cargo.toml",
      ),
      libraryManifest: path.resolve(
        npmWorkspaceRoot,
        "../../sprawl-guard-lib/Cargo.toml",
      ),
      npmVersion: path.join(npmWorkspaceRoot, "package-version.json"),
    },
    {
      kind: "public",
      workspaceRoot: path.resolve(npmWorkspaceRoot, ".."),
      cliManifest: path.resolve(
        npmWorkspaceRoot,
        "../crates/sprawl-guard/Cargo.toml",
      ),
      libraryManifest: path.resolve(
        npmWorkspaceRoot,
        "../crates/sprawl-guard-lib/Cargo.toml",
      ),
      npmVersion: path.join(npmWorkspaceRoot, "package-version.json"),
    },
  ];
  const diagnosedCandidates = candidates.map((candidate) => ({
    layout: candidate,
    missingPaths: [
      path.join(candidate.workspaceRoot, "Cargo.toml"),
      candidate.cliManifest,
      candidate.libraryManifest,
      candidate.npmVersion,
    ].filter((candidatePath) => !isFile(candidatePath)),
  }));
  const matches = diagnosedCandidates.filter(
    ({ missingPaths }) => missingPaths.length === 0,
  );
  if (matches.length === 1 && matches[0] !== undefined) {
    return { kind: "ok", layout: matches[0].layout };
  }
  return {
    kind: "invalid_layout",
    candidates: diagnosedCandidates.map(({ layout, missingPaths }) => ({
      kind: layout.kind,
      missingPaths,
      workspaceRoot: layout.workspaceRoot,
    })),
    reason: matches.length === 0 ? "no_match" : "ambiguous_match",
    scriptPath,
  };
}

function isFile(candidatePath: string): boolean {
  try {
    return fs.statSync(candidatePath).isFile();
  } catch {
    return false;
  }
}

function loadCurrentVersions(
  layout: PublicPackageVersionLayout,
):
  | { readonly kind: "ok"; readonly versions: CurrentVersions }
  | Extract<
      SetPublicPackageVersionResult,
      { readonly kind: "invalid_workspace" }
    > {
  const metadataResult = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps"],
    { cwd: layout.workspaceRoot, encoding: "utf8" },
  );
  if (metadataResult.error !== undefined) {
    return {
      kind: "invalid_workspace",
      detail: `cargo metadata could not start: ${String(metadataResult.error)}`,
    };
  }
  if (metadataResult.status !== 0) {
    return {
      kind: "invalid_workspace",
      detail: `cargo metadata failed: ${metadataResult.stderr.trim()}`,
    };
  }
  const metadata = CARGO_METADATA_SCHEMA.safeParse(
    parseJson(metadataResult.stdout),
  );
  if (!metadata.success) {
    return {
      kind: "invalid_workspace",
      detail: `invalid cargo metadata: ${metadata.error.message}`,
    };
  }
  const cliPackage = uniquePackage(metadata.data.packages, CLI_PACKAGE_NAME);
  const libraryPackage = uniquePackage(
    metadata.data.packages,
    LIBRARY_PACKAGE_NAME,
  );
  if (cliPackage === undefined || libraryPackage === undefined) {
    return {
      kind: "invalid_workspace",
      detail:
        "cargo metadata must contain exactly one public CLI and library package",
    };
  }
  if (cliPackage.version !== libraryPackage.version) {
    return {
      kind: "invalid_workspace",
      detail: `Cargo package versions disagree: ${cliPackage.version} and ${libraryPackage.version}`,
    };
  }
  const libraryDependency = cliPackage.dependencies.filter(
    ({ name }) => name === LIBRARY_PACKAGE_NAME,
  );
  if (
    libraryDependency.length !== 1 ||
    libraryDependency[0]?.req !== `=${cliPackage.version}`
  ) {
    return {
      kind: "invalid_workspace",
      detail: `CLI dependency must require ${LIBRARY_PACKAGE_NAME} =${cliPackage.version}`,
    };
  }
  const npmVersion = readNpmVersion(layout.npmVersion);
  if (npmVersion.kind !== "ok") {
    return npmVersion;
  }
  return {
    kind: "ok",
    versions: {
      cargoVersion: cliPackage.version,
      npmVersion: npmVersion.version,
    },
  };
}

function uniquePackage(
  packages: z.infer<typeof CARGO_METADATA_SCHEMA>["packages"],
  name: PackageName,
): z.infer<typeof CARGO_METADATA_SCHEMA>["packages"][number] | undefined {
  const matches = packages.filter((candidate) => candidate.name === name);
  return matches.length === 1 ? matches[0] : undefined;
}

function readNpmVersion(
  npmVersionPath: string,
):
  | { readonly kind: "ok"; readonly version: PublicPackageVersion }
  | Extract<
      SetPublicPackageVersionResult,
      { readonly kind: "invalid_workspace" }
    > {
  try {
    return { kind: "ok", version: loadPublicPackageVersion(npmVersionPath) };
  } catch (cause) {
    return {
      kind: "invalid_workspace",
      detail:
        cause instanceof PublicPackageVersionFileError
          ? cause.message
          : `failed to load npm package version file: ${String(cause)}`,
    };
  }
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

interface VersionFileContents {
  readonly cliManifest: string;
  readonly libraryManifest: string;
  readonly npmVersion: string;
}

function readVersionFiles(
  paths: PublicPackageVersionLayout,
):
  | { readonly kind: "ok"; readonly files: VersionFileContents }
  | Extract<
      SetPublicPackageVersionResult,
      { readonly kind: "invalid_workspace" }
    > {
  try {
    return {
      kind: "ok",
      files: {
        cliManifest: fs.readFileSync(paths.cliManifest, "utf8"),
        libraryManifest: fs.readFileSync(paths.libraryManifest, "utf8"),
        npmVersion: fs.readFileSync(paths.npmVersion, "utf8"),
      },
    };
  } catch (cause) {
    return {
      kind: "invalid_workspace",
      detail: `failed to read package version source: ${String(cause)}`,
    };
  }
}

function rewriteVersionFiles(args: {
  readonly currentVersion: PublicPackageVersion;
  readonly newVersion: PublicPackageVersion;
  readonly originalFiles: VersionFileContents;
}):
  | { readonly kind: "ok"; readonly files: VersionFileContents }
  | Extract<
      SetPublicPackageVersionResult,
      { readonly kind: "invalid_workspace" }
    > {
  const cliManifest = parseCargoManifest(
    args.originalFiles.cliManifest,
    CLI_MANIFEST_SCHEMA,
    "CLI",
  );
  if (cliManifest.kind !== "ok") {
    return cliManifest;
  }
  const libraryManifest = parseCargoManifest(
    args.originalFiles.libraryManifest,
    LIBRARY_MANIFEST_SCHEMA,
    "library",
  );
  if (libraryManifest.kind !== "ok") {
    return libraryManifest;
  }
  if (
    cliManifest.value.package.version !== args.currentVersion ||
    cliManifest.value.dependencies[LIBRARY_PACKAGE_NAME].version !==
      `=${args.currentVersion}` ||
    libraryManifest.value.package.version !== args.currentVersion
  ) {
    return {
      kind: "invalid_workspace",
      detail: "parsed Cargo manifest versions disagree with Cargo metadata",
    };
  }
  cliManifest.value.package.version = args.newVersion;
  cliManifest.value.dependencies[LIBRARY_PACKAGE_NAME].version =
    `=${args.newVersion}`;
  libraryManifest.value.package.version = args.newVersion;
  try {
    return {
      kind: "ok",
      files: {
        cliManifest: patchToml(
          args.originalFiles.cliManifest,
          cliManifest.value,
        ),
        libraryManifest: patchToml(
          args.originalFiles.libraryManifest,
          libraryManifest.value,
        ),
        npmVersion: `${JSON.stringify({ version: args.newVersion }, null, 2)}\n`,
      },
    };
  } catch (cause) {
    return {
      kind: "invalid_workspace",
      detail: `failed to patch Cargo manifests: ${String(cause)}`,
    };
  }
}

function parseCargoManifest<Schema extends z.ZodType>(
  source: string,
  schema: Schema,
  label: "CLI" | "library",
):
  | { readonly kind: "ok"; readonly value: z.infer<Schema> }
  | Extract<
      SetPublicPackageVersionResult,
      { readonly kind: "invalid_workspace" }
    > {
  let parsedToml: unknown;
  try {
    parsedToml = parseToml(source);
  } catch (cause) {
    return {
      kind: "invalid_workspace",
      detail: `invalid ${label} Cargo manifest TOML: ${String(cause)}`,
    };
  }
  const parsed = schema.safeParse(parsedToml);
  return parsed.success
    ? { kind: "ok", value: parsed.data }
    : {
        kind: "invalid_workspace",
        detail: `invalid ${label} Cargo manifest: ${parsed.error.message}`,
      };
}

/**
 * Replaces every version source with cross-platform copy operations.
 *
 * Same-directory temporary files and backups support rollback after a partial replacement. If
 * recovery fails, the backups are retained and reported instead of being removed.
 */
function writeVersionFiles(
  paths: PublicPackageVersionLayout,
  files: VersionFileContents,
):
  | { readonly kind: "ok" }
  | Extract<
      SetPublicPackageVersionResult,
      { readonly kind: "cleanup_failed" | "write_failed" }
    > {
  const entries = [
    [paths.cliManifest, files.cliManifest],
    [paths.libraryManifest, files.libraryManifest],
    [paths.npmVersion, files.npmVersion],
  ] as const;
  const transactionId = `${process.pid}-${randomUUID()}`;
  const temporaryPaths: string[] = [];
  const backupPaths: string[] = [];
  let replacementsStarted = false;
  let writeFailure: unknown;
  try {
    for (const [destination, contents] of entries) {
      const temporaryPath = `${destination}.tmp-${transactionId}`;
      fs.writeFileSync(temporaryPath, contents, { flag: "wx" });
      temporaryPaths.push(temporaryPath);
    }
    for (const [destination] of entries) {
      const backupPath = `${destination}.backup-${transactionId}`;
      fs.copyFileSync(destination, backupPath, fs.constants.COPYFILE_EXCL);
      backupPaths.push(backupPath);
    }
    for (const [[destination], temporaryPath] of entries.map(
      (entry, index) => [entry, temporaryPaths[index]] as const,
    )) {
      if (temporaryPath === undefined) {
        throw new Error(`missing temporary file for ${destination}`);
      }
      replacementsStarted = true;
      fs.copyFileSync(temporaryPath, destination);
    }
  } catch (cause) {
    writeFailure = cause;
  }

  if (writeFailure === undefined) {
    const cleanup = cleanupAuxiliaryFiles([...temporaryPaths, ...backupPaths]);
    return cleanup.failures.length === 0
      ? { kind: "ok" }
      : {
          kind: "cleanup_failed",
          detail: cleanup.failures.join("; "),
          retainedPaths: cleanup.retainedPaths,
        };
  }

  let rollbackFailure: unknown;
  if (replacementsStarted) {
    try {
      for (const [[destination], backupPath] of entries.map(
        (entry, index) => [entry, backupPaths[index]] as const,
      )) {
        if (backupPath === undefined) {
          throw new Error(`missing backup file for ${destination}`);
        }
        fs.copyFileSync(backupPath, destination);
      }
    } catch (cause) {
      rollbackFailure = cause;
    }
  }
  const cleanup = cleanupAuxiliaryFiles(
    rollbackFailure === undefined
      ? [...temporaryPaths, ...backupPaths]
      : temporaryPaths,
  );
  const retainedPaths = [
    ...(rollbackFailure === undefined ? [] : backupPaths),
    ...cleanup.retainedPaths,
  ];
  const details = [String(writeFailure)];
  if (rollbackFailure !== undefined) {
    details.push(`rollback failed: ${String(rollbackFailure)}`);
  }
  details.push(...cleanup.failures);
  return {
    kind: "write_failed",
    detail: details.join("; "),
    retainedPaths,
  };
}

function cleanupAuxiliaryFiles(paths: readonly string[]): {
  readonly failures: readonly string[];
  readonly retainedPaths: readonly string[];
} {
  const failures: string[] = [];
  const retainedPaths: string[] = [];
  for (const cleanupPath of paths) {
    try {
      fs.rmSync(cleanupPath, { force: true });
    } catch (cause) {
      failures.push(`failed to remove ${cleanupPath}: ${String(cause)}`);
      retainedPaths.push(cleanupPath);
    }
  }
  return { failures, retainedPaths };
}

function uniquePaths(paths: readonly string[]): readonly string[] {
  return [...new Set(paths)];
}

function printFailure(
  result: Exclude<SetPublicPackageVersionResult, { readonly kind: "ok" }>,
): void {
  console.error(JSON.stringify(result));
}

function main(): number {
  const arguments_ = process.argv.slice(2);
  const resolvedLayout = resolvePublicPackageVersionLayout(
    fileURLToPath(import.meta.url),
  );
  if (resolvedLayout.kind !== "ok") {
    console.error(JSON.stringify(resolvedLayout));
    return 1;
  }
  if (arguments_.length === 1 && arguments_[0] === "--check") {
    const result = checkPublicPackageVersionConsistency(resolvedLayout.layout);
    if (result.kind === "ok") {
      return 0;
    }
    printFailure(result);
    return 1;
  }
  const [newVersion, ...extraArguments] = arguments_;
  if (
    newVersion === undefined ||
    extraArguments.length > 0 ||
    newVersion === "--check"
  ) {
    console.error("usage: set-public-version.ts <semver> | --check");
    return 1;
  }
  const result = setPublicPackageVersion({
    layout: resolvedLayout.layout,
    newVersion,
  });
  if (result.kind === "ok") {
    return 0;
  }
  printFailure(result);
  return 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
