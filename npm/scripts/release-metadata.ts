#!/usr/bin/env node

import fs from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { NPM_DIST_TAG_SCHEMA } from "./npm-dist-tag.ts";
import { PACKAGE_VERSION } from "./package-manifests.ts";
import { PUBLIC_PACKAGE_VERSION_SCHEMA } from "./package-version.ts";
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
const RELEASE_EVENT_SCHEMA = z.enum(["push", "release", "workflow_dispatch"]);
interface ResolveReleaseMetadataArgs {
  readonly cargoMetadata: unknown;
  readonly githubEventCommit: string;
  readonly githubEventName: string;
  readonly githubRefName: string;
  readonly githubRefType: string;
  readonly inputReleaseCommit: string;
  readonly inputReleaseTag: string;
  readonly tagCommit: string | null;
  readonly workingTreeCommit: string;
}

interface ResolvedReleaseMetadata {
  readonly kind: "ok";
  readonly releaseCommit: ReleaseCommit;
  readonly releaseTag: ReleaseTag;
  readonly releaseVersion: ReleaseVersion;
}

type ResolveReleaseMetadataResult =
  | ResolvedReleaseMetadata
  | { readonly kind: "missing_release_tag" }
  | { readonly kind: "missing_release_commit" }
  | { readonly kind: "invalid_release_event"; readonly eventName: string }
  | { readonly kind: "invalid_release_commit"; readonly releaseCommit: string }
  | Extract<ParseReleaseTagResult, { readonly kind: "invalid_release_tag" }>
  | { readonly kind: "release_tag_not_found"; readonly releaseTag: ReleaseTag }
  | {
      readonly kind: "release_commit_mismatch";
      readonly expectedCommit: ReleaseCommit;
      readonly tagCommit: string;
    }
  | {
      readonly kind: "release_checkout_mismatch";
      readonly tagCommit: string;
      readonly workingTreeCommit: string;
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
  const requestedTag =
    args.inputReleaseTag.length > 0
      ? args.inputReleaseTag
      : args.githubRefType === "tag"
        ? args.githubRefName
        : null;
  if (requestedTag === null) {
    return { kind: "missing_release_tag" };
  }

  const parsedTag = parseReleaseTag(requestedTag);
  if (parsedTag.kind !== "ok") {
    return parsedTag;
  }
  if (args.tagCommit === null) {
    return {
      kind: "release_tag_not_found",
      releaseTag: parsedTag.releaseTag,
    };
  }
  const event = RELEASE_EVENT_SCHEMA.safeParse(args.githubEventName);
  if (!event.success) {
    return { kind: "invalid_release_event", eventName: args.githubEventName };
  }
  const expectedCommitValue =
    event.data === "workflow_dispatch"
      ? args.inputReleaseCommit
      : args.githubEventCommit;
  const expectedCommit =
    expectedCommitValue.length === 0
      ? null
      : RELEASE_COMMIT_SCHEMA.safeParse(expectedCommitValue);
  if (expectedCommit === null) {
    return { kind: "missing_release_commit" };
  }
  if (!expectedCommit.success) {
    return {
      kind: "invalid_release_commit",
      releaseCommit: expectedCommitValue,
    };
  }
  if (expectedCommit.data !== args.tagCommit) {
    return {
      kind: "release_commit_mismatch",
      expectedCommit: expectedCommit.data,
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
    releaseCommit: expectedCommit.data,
    releaseTag: parsedTag.releaseTag,
    releaseVersion: parsedTag.releaseVersion,
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
  const inputReleaseCommit = process.env.INPUT_RELEASE_COMMIT ?? "";
  const inputReleaseTag = process.env.INPUT_RELEASE_TAG ?? "";
  const githubRefName = process.env.GITHUB_REF_NAME ?? "";
  const githubRefType = process.env.GITHUB_REF_TYPE ?? "";
  const githubEventCommit = process.env.GITHUB_EVENT_COMMIT ?? "";
  const githubEventName = process.env.GITHUB_EVENT_NAME ?? "";
  const requestedTag =
    inputReleaseTag.length > 0
      ? inputReleaseTag
      : githubRefType === "tag"
        ? githubRefName
        : "";
  const parsedTag = parseReleaseTag(requestedTag);
  if (parsedTag.kind !== "ok") {
    printFailure(
      requestedTag.length === 0 ? { kind: "missing_release_tag" } : parsedTag,
    );
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

  const result = resolveReleaseMetadata({
    cargoMetadata: parsedCargoMetadata,
    githubEventCommit,
    githubEventName,
    githubRefName,
    githubRefType,
    inputReleaseTag,
    inputReleaseCommit,
    tagCommit: tagCommit.stdout,
    workingTreeCommit: workingTreeCommit.stdout,
  });
  if (result.kind !== "ok") {
    printFailure(result);
    return 1;
  }

  const npmDistTag = NPM_DIST_TAG_SCHEMA.safeParse(process.env.NPM_DIST_TAG ?? "");
  if (!npmDistTag.success) {
    console.error("NPM_DIST_TAG must contain one validated npm distribution tag");
    return 1;
  }
  const githubDisposition = classifyGitHubRelease(
    result.releaseVersion,
    npmDistTag.data,
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
    case "missing_release_tag":
      console.error("workflow_dispatch runs must provide inputs.release_tag");
      return;
    case "missing_release_commit":
      console.error(
        "workflow_dispatch runs must provide inputs.release_commit",
      );
      return;
    case "invalid_release_event":
      console.error(`unsupported release event: ${failure.eventName}`);
      return;
    case "invalid_release_commit":
      console.error(
        `release commit must be a full lowercase Git SHA: ${failure.releaseCommit}`,
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
