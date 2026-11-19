#!/usr/bin/env node

import fs from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { resolveNpmDistTagSelection } from "./npm-dist-tag.ts";
import { PACKAGE_VERSION } from "./package-manifests.ts";
import { PUBLIC_PACKAGE_VERSION_SCHEMA } from "./package-version.ts";
import {
  pythonPackageVersion,
  type PythonPackageVersion,
  type PythonPackageVersionResult,
} from "./pypi/python-version.ts";
import {
  classifyGitHubRelease,
  parseReleaseTag,
  validateReleaseVersions,
  type ParseReleaseTagResult,
  type ReleaseTag,
  type ReleaseVersion,
  type ValidateReleaseVersionsResult,
} from "./release-policy.ts";
import { RELEASE_COMMIT_SCHEMA, type ReleaseCommit } from "./release-source.ts";

const CARGO_METADATA_SCHEMA = z.object({
  packages: z.array(
    z.object({
      name: z.string().min(1),
      version: PUBLIC_PACKAGE_VERSION_SCHEMA,
    }),
  ),
});
interface ResolveReleaseMetadataArgs {
  readonly cargoMetadata: unknown;
  readonly githubEventCommit: string;
  readonly githubRefName: string;
  readonly githubRefType: string;
  readonly tagCommit: ReleaseCommit | null;
  readonly workingTreeCommit: ReleaseCommit;
}

interface ResolvedReleaseMetadata {
  readonly kind: "ok";
  readonly releaseCommit: ReleaseCommit;
  readonly releaseTag: ReleaseTag;
  readonly releaseVersion: ReleaseVersion;
  /** The release version as PyPI spells it. */
  readonly pythonVersion: PythonPackageVersion;
}

type ResolveReleaseMetadataResult =
  | ResolvedReleaseMetadata
  | {
      readonly kind: "invalid_release_event_commit";
      readonly eventCommit: string;
    }
  | Extract<ParseReleaseTagResult, { readonly kind: "invalid_release_tag" }>
  | Exclude<PythonPackageVersionResult, { readonly kind: "ok" }>
  | {
      readonly kind: "release_ref_not_tag";
      readonly actualRefName: string;
      readonly actualRefType: string;
    }
  | { readonly kind: "release_tag_not_found"; readonly releaseTag: ReleaseTag }
  | {
      readonly kind: "release_commit_mismatch";
      readonly expectedCommit: ReleaseCommit;
      readonly tagCommit: ReleaseCommit;
    }
  | {
      readonly kind: "release_checkout_mismatch";
      readonly tagCommit: ReleaseCommit;
      readonly workingTreeCommit: ReleaseCommit;
    }
  | { readonly kind: "invalid_cargo_metadata"; readonly detail: string }
  | Exclude<ValidateReleaseVersionsResult, { readonly kind: "ok" }>;

type CommandResult =
  | { readonly kind: "ok"; readonly stdout: string }
  | {
      readonly kind: "command_failed";
      readonly command: string;
      readonly detail: string;
    };

/** Resolves release metadata and rejects drift between the tag, checkout, Cargo, and npm. */
export function resolveReleaseMetadata(
  args: ResolveReleaseMetadataArgs,
): ResolveReleaseMetadataResult {
  if (args.githubRefType !== "tag") {
    return {
      kind: "release_ref_not_tag",
      actualRefName: args.githubRefName,
      actualRefType: args.githubRefType,
    };
  }

  const parsedTag = parseReleaseTag(args.githubRefName);
  if (parsedTag.kind !== "ok") {
    return parsedTag;
  }
  const pythonVersion = pythonPackageVersion(parsedTag.releaseVersion);
  if (pythonVersion.kind !== "ok") {
    return pythonVersion;
  }
  if (args.tagCommit === null) {
    return {
      kind: "release_tag_not_found",
      releaseTag: parsedTag.releaseTag,
    };
  }
  const eventCommit = RELEASE_COMMIT_SCHEMA.safeParse(args.githubEventCommit);
  if (!eventCommit.success) {
    return {
      kind: "invalid_release_event_commit",
      eventCommit: args.githubEventCommit,
    };
  }
  if (eventCommit.data !== args.tagCommit) {
    return {
      kind: "release_commit_mismatch",
      expectedCommit: eventCommit.data,
      tagCommit: args.tagCommit,
    };
  }
  if (args.tagCommit !== args.workingTreeCommit) {
    return {
      kind: "release_checkout_mismatch",
      tagCommit: args.tagCommit,
      workingTreeCommit: args.workingTreeCommit,
    };
  }

  const cargoMetadata = CARGO_METADATA_SCHEMA.safeParse(args.cargoMetadata);
  if (!cargoMetadata.success) {
    return {
      kind: "invalid_cargo_metadata",
      detail: z.prettifyError(cargoMetadata.error),
    };
  }

  const versionValidation = validateReleaseVersions({
    cargoPackages: cargoMetadata.data.packages,
    npmVersion: PACKAGE_VERSION,
    releaseVersion: parsedTag.releaseVersion,
  });
  if (versionValidation.kind !== "ok") {
    return versionValidation;
  }

  return {
    kind: "ok",
    releaseCommit: eventCommit.data,
    releaseTag: parsedTag.releaseTag,
    releaseVersion: parsedTag.releaseVersion,
    pythonVersion: pythonVersion.version,
  };
}

function runCommand(command: string, args: readonly string[]): CommandResult {
  const result = spawnSync(command, args, { encoding: "utf8" });
  if (result.error !== undefined || result.status !== 0) {
    return {
      kind: "command_failed",
      command: [command, ...args].join(" "),
      detail: result.error?.message ?? result.stderr.trim(),
    };
  }
  return { kind: "ok", stdout: result.stdout.trim() };
}

function parseJson(text: string): unknown | { readonly kind: "invalid_json" } {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return { kind: "invalid_json" };
  }
}

function main(): number {
  const githubRefName = process.env.GITHUB_REF_NAME ?? "";
  const githubRefType = process.env.GITHUB_REF_TYPE ?? "";
  const githubEventCommit = process.env.GITHUB_EVENT_COMMIT ?? "";
  if (githubRefType !== "tag") {
    printFailure({
      kind: "release_ref_not_tag",
      actualRefName: githubRefName,
      actualRefType: githubRefType,
    });
    return 1;
  }
  const parsedTag = parseReleaseTag(githubRefName);
  if (parsedTag.kind !== "ok") {
    printFailure(parsedTag);
    return 1;
  }

  const tagCommit = runCommand("git", [
    "rev-parse",
    "--verify",
    `${parsedTag.releaseRef}^{commit}`,
  ]);
  const workingTreeCommit = runCommand("git", ["rev-parse", "HEAD"]);
  const cargoMetadata = runCommand("cargo", [
    "metadata",
    "--format-version",
    "1",
    "--no-deps",
  ]);
  const failedCommand = [tagCommit, workingTreeCommit, cargoMetadata].find(
    (commandResult) => commandResult.kind !== "ok",
  );
  if (failedCommand?.kind === "command_failed") {
    console.error(
      `${failedCommand.command} failed: ${failedCommand.detail || "unknown error"}`,
    );
    return 1;
  }
  if (
    tagCommit.kind !== "ok" ||
    workingTreeCommit.kind !== "ok" ||
    cargoMetadata.kind !== "ok"
  ) {
    return 1;
  }

  const parsedCargoMetadata = parseJson(cargoMetadata.stdout);
  if (
    typeof parsedCargoMetadata === "object" &&
    parsedCargoMetadata !== null &&
    "kind" in parsedCargoMetadata &&
    parsedCargoMetadata.kind === "invalid_json"
  ) {
    console.error("cargo metadata returned invalid JSON");
    return 1;
  }
  const parsedTagCommit = RELEASE_COMMIT_SCHEMA.safeParse(tagCommit.stdout);
  const parsedWorkingTreeCommit = RELEASE_COMMIT_SCHEMA.safeParse(
    workingTreeCommit.stdout,
  );
  if (!parsedTagCommit.success || !parsedWorkingTreeCommit.success) {
    console.error("git rev-parse returned an invalid full commit SHA");
    return 1;
  }

  const result = resolveReleaseMetadata({
    cargoMetadata: parsedCargoMetadata,
    githubEventCommit,
    githubRefName,
    githubRefType,
    tagCommit: parsedTagCommit.data,
    workingTreeCommit: parsedWorkingTreeCommit.data,
  });
  if (result.kind !== "ok") {
    printFailure(result);
    return 1;
  }

  const npmDistTag = resolveNpmDistTagSelection({
    customTag: process.env.CUSTOM_NPM_TAG ?? "",
    releaseVersion: result.releaseVersion,
    selection: process.env.NPM_CHANNEL ?? "",
  });
  if (npmDistTag.kind !== "ok") {
    console.error(`NPM_CHANNEL and CUSTOM_NPM_TAG do not select a usable npm channel: ${JSON.stringify(npmDistTag)}`);
    return 1;
  }
  const githubDisposition = classifyGitHubRelease(
    result.releaseVersion,
    npmDistTag.tag,
  );
  const githubPrerelease = githubDisposition.kind === "prerelease";
  const githubLatest = githubDisposition.kind === "stable_latest";

  const githubOutputPath = process.env.GITHUB_OUTPUT;
  if (githubOutputPath === undefined || githubOutputPath.length === 0) {
    console.error("GITHUB_OUTPUT must name the GitHub Actions output file");
    return 1;
  }
  fs.appendFileSync(
    githubOutputPath,
    [
      `release_commit=${result.releaseCommit}`,
      `release_tag=${result.releaseTag}`,
      `release_version=${result.releaseVersion}`,
      `python_version=${result.pythonVersion}`,
      `npm_dist_tag=${npmDistTag.tag}`,
      `github_prerelease=${githubPrerelease}`,
      `github_latest=${githubLatest}`,
      "",
    ].join("\n"),
  );
  return 0;
}

function printFailure(
  failure: Exclude<ResolveReleaseMetadataResult, { kind: "ok" }>,
): void {
  switch (failure.kind) {
    case "invalid_release_event_commit":
      console.error(
        `GitHub event commit must be a full lowercase Git SHA: ${failure.eventCommit}`,
      );
      return;
    case "release_ref_not_tag":
      console.error(
        `workflow must run from a release tag, not ${failure.actualRefType}:${failure.actualRefName}`,
      );
      return;
    case "release_commit_mismatch":
      console.error(
        `release tag commit ${failure.tagCommit} does not match expected commit ${failure.expectedCommit}`,
      );
      return;
    case "invalid_release_tag":
      console.error(
        `release tag must contain a v-prefixed Cargo/npm version: ${failure.releaseTag}`,
      );
      return;
    case "unsupported_prerelease":
      console.error(
        `PyPI has no spelling for the prerelease in ${failure.releaseVersion}; use alpha.N, beta.N, rc.N, or dev.N`,
      );
      return;
    case "release_tag_not_found":
      console.error(`release tag does not exist: ${failure.releaseTag}`);
      return;
    case "release_checkout_mismatch":
      console.error(
        `checked-out commit ${failure.workingTreeCommit} does not match release tag commit ${failure.tagCommit}`,
      );
      return;
    case "invalid_cargo_metadata":
      console.error(
        `cargo metadata did not match its expected schema:\n${failure.detail}`,
      );
      return;
    case "version_mismatch":
      console.error(
        [
          `release version ${failure.releaseVersion} does not match every public package:`,
          ...failure.mismatches.map(
            (mismatch) =>
              `- ${mismatch.packageName}: ${mismatch.actualVersion ?? "missing"}`,
          ),
        ].join("\n"),
      );
      return;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
