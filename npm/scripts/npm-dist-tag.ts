#!/usr/bin/env node

import { fileURLToPath } from "node:url";
import { z } from "zod";

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

const NPM_DIST_TAG_SELECTION_SCHEMA = z.enum([
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

/** Result of resolving the preset/custom release-channel inputs. */
export type ResolveNpmDistTagSelectionResult =
  | { readonly kind: "ok"; readonly tag: NpmDistTag }
  | { readonly kind: "invalid_selection"; readonly value: string }
  | { readonly kind: "missing_custom_tag" }
  | { readonly kind: "invalid_custom_tag"; readonly value: string }
  | {
      readonly kind: "unexpected_custom_tag";
      readonly selection: (typeof PRESET_NPM_DIST_TAGS)[number];
    };

/** Resolves workflow or local preset/custom inputs to one branded npm dist-tag. */
export function resolveNpmDistTagSelection(args: {
  readonly customTag: string;
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
  return {
    kind: "ok",
    tag: NPM_DIST_TAG_SCHEMA.parse(parsedSelection.data),
  };
}

function main(): number {
  const [selection, customTag = "", ...extraArguments] = process.argv.slice(2);
  if (selection === undefined || extraArguments.length > 0) {
    console.error(
      "usage: npm-dist-tag.ts <latest|next|beta|rc|canary|custom> [custom-tag]",
    );
    return 1;
  }
  const result = resolveNpmDistTagSelection({ customTag, selection });
  if (result.kind !== "ok") {
    console.error(JSON.stringify(result));
    return 1;
  }
  console.log(result.tag);
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
