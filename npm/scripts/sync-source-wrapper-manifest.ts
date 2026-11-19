#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { sourceWrapperManifestFile } from "./package-manifests.ts";

type SyncMode = "check" | "write";

type SyncSourceWrapperManifestResult =
  | {
      readonly kind: "ok";
      readonly updatedPaths: readonly string[];
    }
  | {
      readonly kind: "invalid_arguments";
      readonly args: readonly string[];
    }
  | {
      readonly kind: "manifest_out_of_sync";
      readonly relativePath: string;
    }
  | {
      readonly kind: "filesystem_error";
      readonly operation: "read" | "write";
      readonly relativePath: string;
      readonly cause: unknown;
    };

type SyncSourceWrapperManifestFailure = Exclude<
  SyncSourceWrapperManifestResult,
  { readonly kind: "ok" }
>;

const WORKSPACE_ROOT_DIRECTORY = path.resolve(
  fileURLToPath(new URL("..", import.meta.url)),
);

/** Synchronizes the checked-in source wrapper manifest or reports drift. */
export function syncSourceWrapperManifest(args: {
  readonly mode: SyncMode;
  readonly workspaceRootDirectory: string;
}): SyncSourceWrapperManifestResult {
  const manifestFile = sourceWrapperManifestFile();
  const fullPath = path.join(args.workspaceRootDirectory, manifestFile.relativePath);
  const existingText = readExistingManifestFile(fullPath, manifestFile.relativePath);
  if (existingText.kind !== "ok") {
    return existingText;
  }

  if (existingText.text === manifestFile.text) {
    return {
      kind: "ok",
      updatedPaths: [],
    };
  }

  if (args.mode === "check") {
    return {
      kind: "manifest_out_of_sync",
      relativePath: manifestFile.relativePath,
    };
  }

  try {
    fs.mkdirSync(path.dirname(fullPath), { recursive: true });
    fs.writeFileSync(fullPath, manifestFile.text);
  } catch (cause) {
    return {
      kind: "filesystem_error",
      operation: "write",
      relativePath: manifestFile.relativePath,
      cause,
    };
  }

  return {
    kind: "ok",
    updatedPaths: [manifestFile.relativePath],
  };
}

function readExistingManifestFile(
  fullPath: string,
  relativePath: string,
):
  | {
      readonly kind: "ok";
      readonly text: string | null;
    }
  | SyncSourceWrapperManifestFailure {
  try {
    return {
      kind: "ok",
      text: fs.readFileSync(fullPath, "utf8"),
    };
  } catch (cause) {
    const nodeError = cause as NodeJS.ErrnoException | undefined;
    if (nodeError?.code === "ENOENT") {
      return {
        kind: "ok",
        text: null,
      };
    }

    return {
      kind: "filesystem_error",
      operation: "read",
      relativePath,
      cause,
    };
  }
}

/**
 * Reads the mode from `args`, or from the command line when omitted. No arguments write the
 * manifest, and a single `--check` only checks it. Anything else, including a bare `--`, is
 * invalid.
 */
export function parseMode(
  args?: readonly string[],
): SyncMode | Extract<SyncSourceWrapperManifestFailure, { readonly kind: "invalid_arguments" }> {
  const tokens = parseTokens(args);
  if (tokens?.length === 0) {
    return "write";
  }
  return tokens?.length === 1 && tokens[0]?.kind === "option"
    ? "check"
    : { kind: "invalid_arguments", args: args ?? [] };
}

function parseTokens(args: readonly string[] | undefined) {
  try {
    return parseArgs({
      ...(args === undefined ? {} : { args: [...args] }),
      options: { check: { type: "boolean" } },
      tokens: true,
    }).tokens;
  } catch {
    return undefined;
  }
}

function main(): number {
  const parsedMode = parseMode();
  if (typeof parsedMode !== "string") {
    printFailure(parsedMode);
    return 1;
  }

  const result = syncSourceWrapperManifest({
    mode: parsedMode,
    workspaceRootDirectory: WORKSPACE_ROOT_DIRECTORY,
  });

  if (result.kind === "ok") {
    return 0;
  }

  printFailure(result);
  return 1;
}

function printFailure(failure: SyncSourceWrapperManifestFailure): void {
  switch (failure.kind) {
    case "invalid_arguments":
      console.error(
        [
          "usage: sync-source-wrapper-manifest.ts [--check]",
          ...failure.args.map((arg) => `unexpected argument: ${arg}`),
        ].join("\n"),
      );
      return;

    case "manifest_out_of_sync":
      console.error(
        [
          "checked-in source wrapper manifest is out of sync:",
          `- ${failure.relativePath}`,
          "",
          "Run `pnpm run sync:source-wrapper-manifest` in the npm directory.",
        ].join("\n"),
      );
      return;

    case "filesystem_error":
      console.error(
        `failed to ${failure.operation} ${failure.relativePath}: ${String(failure.cause)}`,
      );
      return;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
