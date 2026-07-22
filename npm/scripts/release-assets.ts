#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { absoluteDirectoryPath } from "./path-values.ts";
import {
  expectedReleaseAssetNames,
  parseReleaseTag,
  type ReleaseAssetName,
  type ReleaseTag,
} from "./release-policy.ts";

const GH_RELEASE_ASSETS_SCHEMA = z.object({
  isDraft: z.boolean(),
  assets: z.array(
    z.object({
      name: z.string().min(1),
    }),
  ),
});

/** Result returned by a GitHub CLI process adapter. */
export type GhCommandResult =
  | { readonly kind: "ok"; readonly stdout: string }
  | { readonly kind: "command_failed"; readonly detail: string };

/** Injectable GitHub CLI boundary used by release reconciliation and its tests. */
export type GhRunner = (args: readonly string[]) => GhCommandResult;

type AssetSetMismatch = {
  readonly kind: "asset_set_mismatch";
  readonly actualNames: readonly string[];
  readonly expectedNames: readonly ReleaseAssetName[];
};

type ReleaseAssetsResult =
  | { readonly kind: "ok" }
  | AssetSetMismatch
  | { readonly kind: "invalid_release_tag"; readonly releaseTag: string }
  | { readonly kind: "release_not_draft"; readonly releaseTag: ReleaseTag }
  | { readonly kind: "invalid_gh_response"; readonly detail: string }
  | { readonly kind: "gh_command_failed"; readonly detail: string }
  | {
      readonly kind: "filesystem_error";
      readonly operation: "inspect" | "read" | "write";
      readonly path: string;
      readonly cause: unknown;
    };

/** Result of validating release archives and writing their checksum manifest. */
export type PrepareReleaseAssetsResult =
  | {
      readonly kind: "ok";
      readonly assetNames: readonly ReleaseAssetName[];
    }
  | Exclude<ReleaseAssetsResult, { readonly kind: "ok" }>;

/**
 * Validates the complete target archive set and writes a deterministic SHA-256 manifest.
 *
 * The expected names come from the same runtime-target metadata that renders npm packages, so a
 * target cannot silently disappear from the release checksum contract.
 */
export function prepareReleaseAssets(
  assetDirectoryInput: string,
  releaseVersion: string,
): PrepareReleaseAssetsResult {
  const parsedTag = parseReleaseTag(`v${releaseVersion}`);
  if (parsedTag.kind !== "ok") {
    return parsedTag;
  }

  const assetDirectory = absoluteDirectoryPath(assetDirectoryInput);
  const expectedNames = expectedReleaseAssetNames(parsedTag.releaseVersion);
  const checksumName = expectedNames.find((name) => name.endsWith("-checksums.txt"));
  if (checksumName === undefined) {
    throw new Error("release asset contract must contain a checksum manifest");
  }
  const expectedArchiveNames = expectedNames.filter((name) => name !== checksumName);
  const inspected = inspectRegularFiles(assetDirectory);
  if (inspected.kind !== "ok") {
    return inspected;
  }
  const archiveNames = inspected.names.filter((name) => name !== checksumName);
  const mismatch = compareAssetNames(expectedArchiveNames, archiveNames);
  if (mismatch !== null) {
    return mismatch;
  }

  const checksumLines: string[] = [];
  for (const archiveName of expectedArchiveNames) {
    const archivePath = path.join(assetDirectory, archiveName);
    let contents: Buffer;
    try {
      contents = fs.readFileSync(archivePath);
    } catch (cause) {
      return {
        kind: "filesystem_error",
        operation: "read",
        path: archivePath,
        cause,
      };
    }
    checksumLines.push(
      `${createHash("sha256").update(contents).digest("hex")}  ${archiveName}`,
    );
  }

  const checksumPath = path.join(assetDirectory, checksumName);
  try {
    fs.writeFileSync(checksumPath, `${checksumLines.join("\n")}\n`, {
      flag: "w",
    });
  } catch (cause) {
    return {
      kind: "filesystem_error",
      operation: "write",
      path: checksumPath,
      cause,
    };
  }

  return { kind: "ok", assetNames: expectedNames };
}

/** Replaces every asset on an existing draft release and verifies the final remote asset set. */
export function replaceDraftReleaseAssets(args: {
  readonly assetDirectory: string;
  readonly releaseTag: string;
  readonly runGh?: GhRunner;
}): ReleaseAssetsResult {
  const parsedTag = parseReleaseTag(args.releaseTag);
  if (parsedTag.kind !== "ok") {
    return parsedTag;
  }
  const assetDirectory = absoluteDirectoryPath(args.assetDirectory);
  const expectedNames = expectedReleaseAssetNames(parsedTag.releaseVersion);
  const inspected = inspectRegularFiles(assetDirectory);
  if (inspected.kind !== "ok") {
    return inspected;
  }
  const localMismatch = compareAssetNames(expectedNames, inspected.names);
  if (localMismatch !== null) {
    return localMismatch;
  }

  const runGh = args.runGh ?? runGhCommand;
  const existingAssets = viewDraftReleaseAssets(parsedTag.releaseTag, runGh);
  if (existingAssets.kind !== "ok") {
    return existingAssets;
  }
  for (const assetName of existingAssets.assetNames) {
    const draftState = viewDraftReleaseAssets(parsedTag.releaseTag, runGh);
    if (draftState.kind !== "ok") {
      return draftState;
    }
    const deletion = runGh([
      "release",
      "delete-asset",
      parsedTag.releaseTag,
      "--yes",
      "--",
      assetName,
    ]);
    if (deletion.kind !== "ok") {
      return { kind: "gh_command_failed", detail: deletion.detail };
    }
  }

  const draftState = viewDraftReleaseAssets(parsedTag.releaseTag, runGh);
  if (draftState.kind !== "ok") {
    return draftState;
  }
  const upload = runGh([
    "release",
    "upload",
    parsedTag.releaseTag,
    ...expectedNames.map((name) => path.join(assetDirectory, name)),
  ]);
  if (upload.kind !== "ok") {
    return { kind: "gh_command_failed", detail: upload.detail };
  }

  const uploadedAssets = viewDraftReleaseAssets(parsedTag.releaseTag, runGh);
  if (uploadedAssets.kind !== "ok") {
    return uploadedAssets;
  }
  return compareAssetNames(expectedNames, uploadedAssets.assetNames) ?? { kind: "ok" };
}

function inspectRegularFiles(
  directory: string,
):
  | { readonly kind: "ok"; readonly names: readonly string[] }
  | Extract<ReleaseAssetsResult, { readonly kind: "filesystem_error" }> {
  try {
    const entries = fs.readdirSync(directory, { withFileTypes: true });
    const nonFiles = entries.filter((entry) => !entry.isFile());
    if (nonFiles.length > 0) {
      return {
        kind: "filesystem_error",
        operation: "inspect",
        path: path.join(directory, nonFiles[0]?.name ?? ""),
        cause: new Error("release asset directory may contain only regular files"),
      };
    }
    return {
      kind: "ok",
      names: entries.map((entry) => entry.name).sort(),
    };
  } catch (cause) {
    return {
      kind: "filesystem_error",
      operation: "inspect",
      path: directory,
      cause,
    };
  }
}

function compareAssetNames(
  expectedNames: readonly ReleaseAssetName[],
  actualNames: readonly string[],
): AssetSetMismatch | null {
  const sortedExpectedNames = [...expectedNames].sort();
  const sortedActualNames = [...actualNames].sort();
  return JSON.stringify(sortedExpectedNames) === JSON.stringify(sortedActualNames)
    ? null
    : {
        kind: "asset_set_mismatch",
        actualNames: sortedActualNames,
        expectedNames: sortedExpectedNames,
      };
}

function viewDraftReleaseAssets(
  releaseTag: ReleaseTag,
  runGh: GhRunner,
):
  | { readonly kind: "ok"; readonly assetNames: readonly string[] }
  | Extract<
      ReleaseAssetsResult,
      {
        readonly kind:
          | "invalid_gh_response"
          | "gh_command_failed"
          | "release_not_draft";
      }
    > {
  const response = runGh([
    "release",
    "view",
    releaseTag,
    "--json",
    "assets,isDraft",
  ]);
  if (response.kind !== "ok") {
    return { kind: "gh_command_failed", detail: response.detail };
  }
  let json: unknown;
  try {
    json = JSON.parse(response.stdout) as unknown;
  } catch {
    return { kind: "invalid_gh_response", detail: "response was not JSON" };
  }
  const parsed = GH_RELEASE_ASSETS_SCHEMA.safeParse(json);
  if (!parsed.success) {
    return {
      kind: "invalid_gh_response",
      detail: z.prettifyError(parsed.error),
    };
  }
  if (!parsed.data.isDraft) {
    return { kind: "release_not_draft", releaseTag };
  }
  return {
    kind: "ok",
    assetNames: parsed.data.assets.map((asset) => asset.name),
  };
}

function runGhCommand(args: readonly string[]): GhCommandResult {
  const result = spawnSync("gh", args, { encoding: "utf8" });
  return result.error === undefined && result.status === 0
    ? { kind: "ok", stdout: result.stdout }
    : {
        kind: "command_failed",
        detail: result.error?.message ?? result.stderr.trim(),
      };
}

function main(): number {
  const [command, firstArgument, secondArgument] = process.argv.slice(2);
  const result =
    command === "prepare" && firstArgument !== undefined && secondArgument !== undefined
      ? prepareReleaseAssets(firstArgument, secondArgument)
      : command === "replace" && firstArgument !== undefined && secondArgument !== undefined
        ? replaceDraftReleaseAssets({
            assetDirectory: secondArgument,
            releaseTag: firstArgument,
          })
        : null;
  if (result === null) {
    console.error(
      "usage: release-assets.ts prepare <asset-directory> <release-version> | replace <release-tag> <asset-directory>",
    );
    return 1;
  }
  if (result.kind === "ok") {
    return 0;
  }
  console.error(formatFailure(result));
  return 1;
}

function formatFailure(failure: Exclude<PrepareReleaseAssetsResult, { kind: "ok" }>): string {
  switch (failure.kind) {
    case "asset_set_mismatch":
      return [
        "release asset set did not match the expected target set",
        `expected: ${failure.expectedNames.join(", ")}`,
        `actual: ${failure.actualNames.join(", ")}`,
      ].join("\n");
    case "invalid_release_tag":
      return `invalid release tag: ${failure.releaseTag}`;
    case "release_not_draft":
      return `refusing to replace assets on non-draft release ${failure.releaseTag}`;
    case "invalid_gh_response":
      return `GitHub CLI returned an invalid response: ${failure.detail}`;
    case "gh_command_failed":
      return `GitHub CLI command failed: ${failure.detail}`;
    case "filesystem_error":
      return `failed to ${failure.operation} ${failure.path}: ${String(failure.cause)}`;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
