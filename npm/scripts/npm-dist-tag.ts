import { prerelease } from "semver";
import { z } from "zod";
import type { PublicPackageVersion } from "./package-version.ts";

const MAXIMUM_NPM_DIST_TAG_LENGTH = 64;
const CUSTOM_NPM_DIST_TAG_PATTERN = /^(?!v)(?!x(?:$|[.-]))[a-z][a-z0-9._-]*$/u;

/** npm channels offered directly by release entry points. */
export const PRESET_NPM_DIST_TAGS = [
  "latest",
  "next",
  "beta",
  "rc",
  "canary",
] as const;

/**
 * Prerelease labels whose npm channel has the same name, so `auto` can pick it. Alpha and dev
 * prereleases have no matching channel, so they need an explicit one.
 */
const AUTOMATIC_PRERELEASE_CHANNELS = ["beta", "rc"] as const;

const NPM_DIST_TAG_SELECTION_SCHEMA = z.enum([
  "auto",
  ...PRESET_NPM_DIST_TAGS,
  "custom",
]);

/** Validated npm distribution tag recorded in a publication candidate. */
export const NPM_DIST_TAG_SCHEMA = z
  .string()
  .max(MAXIMUM_NPM_DIST_TAG_LENGTH)
  .regex(CUSTOM_NPM_DIST_TAG_PATTERN)
  .brand<"NpmDistTag">();

/** npm distribution tag that is safe to pass to `npm publish --tag`. */
export type NpmDistTag = z.infer<typeof NPM_DIST_TAG_SCHEMA>;

/** Result of resolving the release-channel inputs. */
export type ResolveNpmDistTagSelectionResult =
  | { readonly kind: "ok"; readonly tag: NpmDistTag }
  | { readonly kind: "invalid_selection"; readonly value: string }
  | { readonly kind: "missing_custom_tag" }
  | { readonly kind: "invalid_custom_tag"; readonly value: string }
  | {
      readonly kind: "unexpected_custom_tag";
      readonly selection: Exclude<z.infer<typeof NPM_DIST_TAG_SELECTION_SCHEMA>, "custom">;
    }
  | {
      /** `auto` was selected for a prerelease whose label names no npm channel. */
      readonly kind: "no_automatic_channel";
      readonly releaseVersion: PublicPackageVersion;
    };

/**
 * Resolves workflow or local channel inputs to one branded npm dist-tag. `auto` picks `latest`
 * for a stable release and the channel named by a beta or rc prerelease's label.
 */
export function resolveNpmDistTagSelection(args: {
  readonly customTag: string;
  readonly releaseVersion: PublicPackageVersion;
  readonly selection: string;
}): ResolveNpmDistTagSelectionResult {
  const parsedSelection = NPM_DIST_TAG_SELECTION_SCHEMA.safeParse(
    args.selection,
  );
  if (!parsedSelection.success) {
    return { kind: "invalid_selection", value: args.selection };
  }
  if (parsedSelection.data === "custom") {
    if (args.customTag.length === 0) {
      return { kind: "missing_custom_tag" };
    }
    const parsedCustomTag = NPM_DIST_TAG_SCHEMA.safeParse(args.customTag);
    return parsedCustomTag.success
      ? { kind: "ok", tag: parsedCustomTag.data }
      : { kind: "invalid_custom_tag", value: args.customTag };
  }
  if (args.customTag.length > 0) {
    return {
      kind: "unexpected_custom_tag",
      selection: parsedSelection.data,
    };
  }
  if (parsedSelection.data === "auto") {
    return automaticNpmDistTag(args.releaseVersion);
  }
  return {
    kind: "ok",
    tag: NPM_DIST_TAG_SCHEMA.parse(parsedSelection.data),
  };
}

function automaticNpmDistTag(
  releaseVersion: PublicPackageVersion,
): ResolveNpmDistTagSelectionResult {
  const labels = prerelease(releaseVersion);
  if (labels === null) {
    return { kind: "ok", tag: NPM_DIST_TAG_SCHEMA.parse("latest") };
  }
  const channel = AUTOMATIC_PRERELEASE_CHANNELS.find(
    (candidate) => candidate === labels[0],
  );
  return channel === undefined
    ? { kind: "no_automatic_channel", releaseVersion }
    : { kind: "ok", tag: NPM_DIST_TAG_SCHEMA.parse(channel) };
}
