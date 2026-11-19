#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolveNpmDistTagSelection } from "./npm-dist-tag.ts";
import { parseReleaseTag } from "./release-policy.ts";

const PUBLISH_NPM_PACKAGES_SCRIPT = fileURLToPath(
  new URL("./publish-npm-packages.ts", import.meta.url),
);

export type ResolveNpmChannelPublicationResult =
  | { readonly kind: "ok"; readonly childArguments: readonly string[] }
  | { readonly kind: "invalid_arguments" }
  | { readonly kind: "invalid_release_tag"; readonly releaseTag: string }
  | {
      readonly kind: "invalid_channel";
      readonly detail: Exclude<
        ReturnType<typeof resolveNpmDistTagSelection>,
        { readonly kind: "ok" }
      >;
    };

/** Resolves a preset or custom channel into the low-level publication command. */
export function resolveNpmChannelPublication(
  args: readonly string[],
): ResolveNpmChannelPublicationResult {
  if (args.length < 4 || args.length > 5) {
    return { kind: "invalid_arguments" };
  }
  const [candidateDirectory, releaseCommit, releaseTag, selection, customTag = ""] = args;
  if (
    candidateDirectory === undefined
    || releaseCommit === undefined
    || releaseTag === undefined
    || selection === undefined
  ) {
    return { kind: "invalid_arguments" };
  }
  const parsedTag = parseReleaseTag(releaseTag);
  if (parsedTag.kind !== "ok") {
    return parsedTag;
  }
  const channel = resolveNpmDistTagSelection({
    customTag,
    releaseVersion: parsedTag.releaseVersion,
    selection,
  });
  return channel.kind === "ok"
    ? {
        kind: "ok",
        childArguments: [
          PUBLISH_NPM_PACKAGES_SCRIPT,
          "publish",
          candidateDirectory,
          releaseCommit,
          releaseTag,
          channel.tag,
        ],
      }
    : { kind: "invalid_channel", detail: channel };
}

function main(): number {
  const resolved = resolveNpmChannelPublication(process.argv.slice(2));
  if (resolved.kind !== "ok") {
    console.error(
      "usage: publish-npm-channel.ts <candidate-directory> <full-commit> <v-tag> <auto|latest|next|beta|rc|canary|custom> [custom-tag]",
    );
    console.error(JSON.stringify(resolved));
    return 1;
  }
  const published = spawnSync(process.execPath, resolved.childArguments, {
    stdio: "inherit",
  });
  return published.status ?? 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
