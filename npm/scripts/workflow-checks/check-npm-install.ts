#!/usr/bin/env node

/**
 * Installs Sprawl Guard's npm wrapper into a new project and checks that it runs.
 *
 * The release workflow runs this with Node.js 22 on fresh runners and in a bare Node.js
 * container, before any workspace dependencies are installed, so it uses only Node.js 22
 * built-ins and validates its arguments without Zod.
 */

import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { RUNTIME_PACKAGE_DEFINITIONS } from "../../sprawl-guard/src/lib/runtime-packages.ts";
import {
  type CapturedCommandResult,
  type CommandResult,
  runNodeScript,
  runNpm,
} from "./npm-install-processes.ts";

const WRAPPER_PACKAGE_NAME = "@kpozin/sprawl-guard";
const RUNTIME_PACKAGE_PREFIX = `${WRAPPER_PACKAGE_NAME}-`;
/** The wrapper's `bin` entry point, relative to its installed package directory. */
export const WRAPPER_ENTRY_POINT = "dist/bin/sprawl-guard.js";
const RUNTIME_PACKAGE_PATTERN = /^@kpozin\/sprawl-guard-[a-z0-9]+(?:-[a-z0-9]+)*$/u;
const VERSION_PATTERN = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/u;
const DIST_TAG_PATTERN = /^[a-z][a-z0-9._-]*$/u;
/** Maximum clean-install size of the wrapper, dependencies, and metadata beyond CLI payloads. */
const MAX_INSTALL_OVERHEAD_BYTES = 512 * 1024;

/**
 * How many times a registry install is tried. npm's CDN can serve an older dist-tag or miss a
 * new archive for several minutes after the publisher sees the release.
 */
export const REGISTRY_INSTALL_ATTEMPTS = 30;
const REGISTRY_INSTALL_INTERVAL_MILLISECONDS = 20_000;

// These brands mirror the workspace's package-version and dist-tag schemas, which this
// built-ins-only script can't import.

/** An absolute filesystem path. */
export type AbsolutePath = string & { readonly __brand: "AbsolutePath" };
/** A Sprawl Guard platform package name, such as `@kpozin/sprawl-guard-linux-x64-musl`. */
export type RuntimePackageName = string & { readonly __brand: "RuntimePackageName" };
/** A SemVer package version without a `v` prefix. */
export type PackageVersion = string & { readonly __brand: "PackageVersion" };
/** An npm dist-tag, such as `latest`. */
export type DistTag = string & { readonly __brand: "DistTag" };

/**
 * Where the packages come from.
 *
 * Candidate archives are installed exactly, with optional dependencies omitted. From the
 * registry, npm either picks the platform package itself (`automatic`) or installs the named
 * one with optional dependencies omitted (`explicit`), as a platform without a native package
 * would.
 */
export type InstallSource =
  | { readonly kind: "candidate"; readonly directory: AbsolutePath }
  | {
      readonly kind: "registry";
      readonly distTag: DistTag;
      readonly runtimeInstallation: "automatic" | "explicit";
    };

/** One install check. */
export interface NpmInstallCheck {
  readonly projectDirectory: AbsolutePath;
  readonly runtimePackageName: RuntimePackageName;
  readonly source: InstallSource;
  readonly version: PackageVersion;
}

/** Side effects that tests replace. */
export interface NpmInstallCheckOperations {
  /** Runs npm and streams its stderr, so a slow install shows progress as it happens. */
  readonly runNpm: (args: readonly string[]) => CommandResult;
  /** Runs an installed JavaScript entry point with Node.js and returns its stderr. */
  readonly runEntryPoint: (
    entryPoint: AbsolutePath,
    argument: WrapperArgument,
  ) => CapturedCommandResult;
  readonly sleepBeforeRetry: () => Promise<void>;
}

/** One of the two packages that an install check installs. */
type CheckedPackageName = typeof WRAPPER_PACKAGE_NAME | RuntimePackageName;

/** Why an install did not produce the release. A registry install may still catch up. */
type InstallFailure =
  | { readonly kind: "install_failed"; readonly detail: string }
  | { readonly kind: "package_missing"; readonly packageName: CheckedPackageName }
  | {
      readonly kind: "version_mismatch";
      /** The version in the installed manifest, which need not be valid SemVer. */
      readonly installedVersion: string;
      readonly packageName: CheckedPackageName;
    };

/** Logical sizes of installed regular files and of the files other than expected CLI payloads. */
interface InstallFootprint {
  readonly totalBytes: number;
  readonly overheadBytes: number;
}

/** An argument that the check runs the installed wrapper with. */
export type WrapperArgument = "--help" | "--version";

/** The result of an install check. */
export type NpmInstallCheckResult =
  | { readonly kind: "ok"; readonly footprint: InstallFootprint }
  | { readonly kind: "project_exists"; readonly projectDirectory: AbsolutePath }
  | InstallFailure
  | { readonly kind: "footprint_budget_exceeded"; readonly footprint: InstallFootprint }
  | { readonly kind: "command_failed"; readonly argument: WrapperArgument; readonly detail: string }
  | { readonly kind: "unexpected_stderr"; readonly argument: WrapperArgument; readonly stderr: string }
  | { readonly kind: "unexpected_version_output"; readonly output: string }
  /** The entry point runs, but npm can't run it through the project's `sprawl-guard` bin link. */
  | { readonly kind: "bin_link_failed"; readonly detail: string };

/** Installs the packages, retrying registry installs, then runs the installed wrapper. */
export async function checkNpmInstall(
  check: NpmInstallCheck,
  operations: NpmInstallCheckOperations,
): Promise<NpmInstallCheckResult> {
  if (existsSync(check.projectDirectory)) {
    return { kind: "project_exists", projectDirectory: check.projectDirectory };
  }
  const attempts = check.source.kind === "registry" ? REGISTRY_INSTALL_ATTEMPTS : 1;
  let installed: InstallFailure | { readonly kind: "ok" } = {
    kind: "install_failed",
    detail: "no install attempted",
  };
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    installed = installOnce(check, operations);
    if (installed.kind === "ok") {
      break;
    }
    if (attempt < attempts) {
      console.error(`${describeFailure(installed)}; retrying (attempt ${attempt + 1} of ${attempts})`);
      rmSync(check.projectDirectory, { force: true, recursive: true });
      await operations.sleepBeforeRetry();
    }
  }
  if (installed.kind !== "ok") {
    return installed;
  }
  const footprint = measureInstallFootprint(check);
  if (footprint.overheadBytes > MAX_INSTALL_OVERHEAD_BYTES) {
    return { kind: "footprint_budget_exceeded", footprint };
  }
  const wrapper = runInstalledWrapper(check, operations);
  return wrapper.kind === "passed" ? { kind: "ok", footprint } : wrapper;
}

/** Counts installed files once and budgets separately for everything beyond the CLI payloads. */
function measureInstallFootprint(check: NpmInstallCheck): InstallFootprint {
  const modules = path.join(check.projectDirectory, "node_modules");
  const automatic =
    check.source.kind === "registry" && check.source.runtimeInstallation === "automatic";
  const payloadPaths = new Set(
    RUNTIME_PACKAGE_DEFINITIONS.filter(
      (definition) =>
        definition.packageName === check.runtimePackageName ||
        (automatic && definition.kind === "wasi"),
    ).map((definition) => path.join(modules, definition.packageName, definition.payloadPath)),
  );
  let totalBytes = 0;
  let payloadBytes = 0;
  for (const entry of readdirSync(modules, { recursive: true, withFileTypes: true })) {
    if (!entry.isFile()) {
      continue;
    }
    const file = path.join(entry.parentPath, entry.name);
    const { size } = statSync(file);
    totalBytes += size;
    if (payloadPaths.has(file)) {
      payloadBytes += size;
    }
  }
  return { totalBytes, overheadBytes: totalBytes - payloadBytes };
}

/** Installs into a new project and confirms that both packages have the release version. */
function installOnce(
  check: NpmInstallCheck,
  operations: NpmInstallCheckOperations,
): InstallFailure | { readonly kind: "ok" } {
  mkdirSync(check.projectDirectory, { recursive: true });
  writeFileSync(path.join(check.projectDirectory, "package.json"), '{"private":true}\n');
  const installed = operations.runNpm([
    "--prefix",
    check.projectDirectory,
    "install",
    "--save-exact",
    ...installArguments(check),
  ]);
  if (installed.kind === "failed") {
    return { kind: "install_failed", detail: installed.detail };
  }
  for (const packageName of [WRAPPER_PACKAGE_NAME, check.runtimePackageName] as const) {
    const installedVersion = readInstalledVersion(check.projectDirectory, packageName);
    if (installedVersion === null) {
      return { kind: "package_missing", packageName };
    }
    if (installedVersion !== check.version) {
      return { kind: "version_mismatch", installedVersion, packageName };
    }
  }
  return { kind: "ok" };
}

/** Returns the npm install flags and package specifiers for the check's source. */
function installArguments(check: NpmInstallCheck): readonly string[] {
  switch (check.source.kind) {
    case "candidate": {
      const runtimeSlug = check.runtimePackageName.slice(RUNTIME_PACKAGE_PREFIX.length);
      return [
        "--omit=optional",
        path.join(check.source.directory, `kpozin-sprawl-guard-${check.version}.tgz`),
        path.join(
          check.source.directory,
          `kpozin-sprawl-guard-${runtimeSlug}-${check.version}.tgz`,
        ),
      ];
    }
    case "registry": {
      const wrapper = `${WRAPPER_PACKAGE_NAME}@${check.source.distTag}`;
      // `--prefer-online` because npm otherwise reuses cached registry metadata for minutes, so a
      // retry would see the same stale dist-tag.
      switch (check.source.runtimeInstallation) {
        case "automatic":
          return ["--prefer-online", wrapper];
        case "explicit":
          return [
            "--prefer-online",
            "--omit=optional",
            wrapper,
            `${check.runtimePackageName}@${check.version}`,
          ];
      }
    }
  }
}

/** Reads the version of a package installed at the top of the project's `node_modules`. */
function readInstalledVersion(
  projectDirectory: AbsolutePath,
  packageName: CheckedPackageName,
): string | null {
  const manifestPath = path.join(
    projectDirectory,
    "node_modules",
    ...packageName.split("/"),
    "package.json",
  );
  if (!existsSync(manifestPath)) {
    return null;
  }
  const manifest: unknown = JSON.parse(readFileSync(manifestPath, "utf8"));
  return typeof manifest === "object" &&
    manifest !== null &&
    "version" in manifest &&
    typeof manifest.version === "string"
    ? manifest.version
    : null;
}

/** A wrapper run's failure, or its stdout when it exited cleanly and wrote nothing to stderr. */
type WrapperRun =
  | Extract<NpmInstallCheckResult, { kind: "command_failed" | "unexpected_stderr" }>
  | { readonly kind: "ran"; readonly stdout: string };

/**
 * Runs the installed wrapper's entry point with Node.js. Any stderr fails the run, because a
 * loader or runtime warning there, such as a missing glibc version, reaches every user. Running it
 * without npm keeps npm's own warnings, such as an unknown config notice, out of that stderr.
 */
function runWrapper(
  check: NpmInstallCheck,
  operations: NpmInstallCheckOperations,
  argument: WrapperArgument,
): WrapperRun {
  const entryPoint = path.join(
    check.projectDirectory,
    "node_modules",
    WRAPPER_PACKAGE_NAME,
    WRAPPER_ENTRY_POINT,
  ) as AbsolutePath;
  const result = operations.runEntryPoint(entryPoint, argument);
  if (result.kind === "failed") {
    return { kind: "command_failed", argument, detail: result.detail };
  }
  if (result.stderr !== "") {
    return { kind: "unexpected_stderr", argument, stderr: result.stderr };
  }
  return { kind: "ran", stdout: result.stdout };
}

/** Runs the installed wrapper's `--version` and `--help`, then runs it through npm's bin link. */
function runInstalledWrapper(
  check: NpmInstallCheck,
  operations: NpmInstallCheckOperations,
):
  | Extract<
    NpmInstallCheckResult,
    {
      kind: "bin_link_failed" | "command_failed" | "unexpected_stderr" | "unexpected_version_output";
    }
  >
  | { readonly kind: "passed" } {
  const version = runWrapper(check, operations, "--version");
  if (version.kind !== "ran") {
    return version;
  }
  const output = version.stdout.trim();
  if (output !== `sprawl-guard ${check.version}`) {
    return { kind: "unexpected_version_output", output };
  }
  const help = runWrapper(check, operations, "--help");
  if (help.kind !== "ran") {
    return help;
  }
  const linked = operations.runNpm([
    "--prefix",
    check.projectDirectory,
    "exec",
    "--",
    "sprawl-guard",
    "--help",
  ]);
  return linked.kind === "ok"
    ? { kind: "passed" }
    : { kind: "bin_link_failed", detail: linked.detail };
}

/** Formats the measured size and budget for release logs. */
function describeFootprint({ totalBytes, overheadBytes }: InstallFootprint): string {
  return `node_modules: ${totalBytes} bytes total, ${overheadBytes} bytes outside CLI payloads (budget ${MAX_INSTALL_OVERHEAD_BYTES} bytes)`;
}

/** Explains a failed check in one line. */
function describeFailure(failure: Exclude<NpmInstallCheckResult, { readonly kind: "ok" }>): string {
  switch (failure.kind) {
    case "project_exists":
      return `project directory already exists: ${failure.projectDirectory}`;
    case "install_failed":
      return `npm install failed: ${failure.detail}`;
    case "package_missing":
      return `npm did not install ${failure.packageName}`;
    case "version_mismatch":
      return `npm installed ${failure.packageName}@${failure.installedVersion}`;
    case "footprint_budget_exceeded":
      return `npm install exceeds the footprint budget: ${describeFootprint(failure.footprint)}`;
    case "command_failed":
      return `sprawl-guard ${failure.argument} failed: ${failure.detail}`;
    case "unexpected_stderr":
      return `sprawl-guard ${failure.argument} wrote to stderr: "${failure.stderr.trim()}"`;
    case "unexpected_version_output":
      return `sprawl-guard --version printed "${failure.output}"`;
    case "bin_link_failed":
      return `npm exec could not run the installed sprawl-guard bin link: ${failure.detail}`;
  }
}

/** The result of parsing command-line arguments. */
export type ParseNpmInstallCheckArgumentsResult =
  | { readonly kind: "ok"; readonly check: NpmInstallCheck }
  | { readonly kind: "invalid_arguments"; readonly detail: string };

const USAGE =
  "usage: check-npm-install.ts --project DIR --version VERSION --runtime-package NAME " +
  "(--candidate-directory DIR | --dist-tag TAG --runtime-installation automatic|explicit)";

/** Parses and validates the command line. */
export function parseNpmInstallCheckArguments(
  argv: readonly string[],
): ParseNpmInstallCheckArgumentsResult {
  let values;
  try {
    ({ values } = parseArgs({
      args: [...argv],
      options: {
        "candidate-directory": { type: "string" },
        "dist-tag": { type: "string" },
        project: { type: "string" },
        "runtime-installation": { type: "string" },
        "runtime-package": { type: "string" },
        version: { type: "string" },
      },
      strict: true,
    }));
  } catch (error) {
    return { kind: "invalid_arguments", detail: String(error) };
  }
  const invalid = (detail: string): ParseNpmInstallCheckArgumentsResult => ({
    kind: "invalid_arguments",
    detail: `${detail}\n${USAGE}`,
  });

  const { project, version } = values;
  const runtimePackage = values["runtime-package"];
  if (project === undefined || version === undefined || runtimePackage === undefined) {
    return invalid("--project, --version, and --runtime-package are required");
  }
  if (!VERSION_PATTERN.test(version)) {
    return invalid(`--version must be a SemVer version without a v prefix: ${version}`);
  }
  if (!RUNTIME_PACKAGE_PATTERN.test(runtimePackage)) {
    return invalid(`--runtime-package must name a Sprawl Guard platform package: ${runtimePackage}`);
  }

  const candidateDirectory = values["candidate-directory"];
  const distTag = values["dist-tag"];
  const runtimeInstallation = values["runtime-installation"];
  let source: InstallSource;
  if (candidateDirectory !== undefined && distTag === undefined && runtimeInstallation === undefined) {
    source = { kind: "candidate", directory: path.resolve(candidateDirectory) as AbsolutePath };
  } else if (candidateDirectory === undefined && distTag !== undefined) {
    if (!DIST_TAG_PATTERN.test(distTag)) {
      return invalid(`--dist-tag must be an npm dist-tag: ${distTag}`);
    }
    if (runtimeInstallation !== "automatic" && runtimeInstallation !== "explicit") {
      return invalid("--dist-tag needs --runtime-installation automatic or explicit");
    }
    source = { kind: "registry", distTag: distTag as DistTag, runtimeInstallation };
  } else {
    return invalid("give either --candidate-directory or --dist-tag with --runtime-installation");
  }

  return {
    kind: "ok",
    check: {
      projectDirectory: path.resolve(project) as AbsolutePath,
      runtimePackageName: runtimePackage as RuntimePackageName,
      source,
      version: version as PackageVersion,
    },
  };
}

async function main(): Promise<number> {
  const parsed = parseNpmInstallCheckArguments(process.argv.slice(2));
  if (parsed.kind === "invalid_arguments") {
    console.error(parsed.detail);
    return 2;
  }
  const result = await checkNpmInstall(parsed.check, {
    runNpm,
    runEntryPoint: (entryPoint, argument) => runNodeScript(entryPoint, [argument]),
    sleepBeforeRetry: () => sleep(REGISTRY_INSTALL_INTERVAL_MILLISECONDS),
  });
  if (result.kind !== "ok") {
    console.error(describeFailure(result));
    return 1;
  }
  console.log(describeFootprint(result.footprint));
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(await main());
}
