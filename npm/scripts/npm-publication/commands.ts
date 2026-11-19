/**
 * The external commands the npm publisher runs: `npm view` and `npm publish` against the registry,
 * and `pnpm` and `tar` for packing and inspecting archives. `registry-checks.ts` adds retries and
 * visibility polling on top of the registry calls.
 */
import { spawnSync } from "node:child_process";
import { z } from "zod";
import type { NpmDistTag } from "../npm-dist-tag.ts";
import { PACKAGE_VERSION } from "../package-manifests.ts";
import { PUBLIC_PACKAGE_VERSION_SCHEMA } from "../package-version.ts";
import type {
  NpmPackageName,
  PublishArchiveResult,
  RemoteNpmDistTagInspection,
  RemotePackageIntegrityInspection,
} from "./model.ts";
import { validateArchiveIntegrity } from "./validation.ts";
import {
  NPM_PACKAGE_INTEGRITY_SCHEMA,
  NPM_REGISTRY_URL,
  type AbsoluteDirectoryPath,
  type NpmArchivePath,
  type NpmPackageIntegrity,
} from "./values.ts";

/** The error code npm reports when the registry has no such package, version or dist-tag. */
const NPM_NOT_FOUND_CODE = "E404";
/** npm failures that a later retry can get past, such as timeouts and rate limits. */
const TRANSIENT_NPM_FAILURE_PATTERN =
  /(?:EAI_AGAIN|ECONNRESET|ETIMEDOUT|ERR_SOCKET_TIMEOUT|E429|E50[0234]|HTTP (?:408|429|5\d\d))/u;
const CAPTURED_COMMAND_MAXIMUM_OUTPUT_BYTES = 1_048_576;
const CAPTURED_COMMAND_TIMEOUT_MILLISECONDS = 30_000;
const NPM_PUBLISH_COMMAND_TIMEOUT_MILLISECONDS = 600_000;

/** The captured output of a command that exited 0, or why it did not. */
export type CommandResult =
  | { readonly kind: "ok"; readonly stdout: string }
  | { readonly kind: "failed"; readonly detail: string };

/** Reads the integrity the registry holds for this release's version of a package. */
export function inspectRemoteIntegrity(
  packageName: NpmPackageName,
): RemotePackageIntegrityInspection {
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

/** Reads the version a dist-tag of a package points to. */
export function inspectRemoteDistTag(
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

/** Publishes an archive after checking that it still has the integrity the candidate recorded. */
export function publishArchive(
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

/** Parses JSON command output, returning `undefined` for text that is not JSON. */
export function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

/** Runs a command and captures its output, with a bounded output size and running time. */
export function runCommand(
  command: "npm" | "pnpm" | "tar",
  args: readonly string[],
  cwd?: AbsoluteDirectoryPath,
): CommandResult {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: CAPTURED_COMMAND_MAXIMUM_OUTPUT_BYTES,
    stdio: ["ignore", "pipe", "pipe"],
    timeout: CAPTURED_COMMAND_TIMEOUT_MILLISECONDS,
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
