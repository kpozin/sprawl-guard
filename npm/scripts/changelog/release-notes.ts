#!/usr/bin/env node

import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import {
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  type PublicPackageVersion,
} from "../package-version.ts";
import {
  fencedLines,
  LIST_ITEM_PREFIX,
  proseLines,
  sectionBody,
  versionHeadingPrefix,
} from "./sections.ts";

/** The changelog at the public repository root. */
const CHANGELOG_URL = new URL("../../../CHANGELOG.md", import.meta.url);

/** Result of reading one release's notes from the changelog. */
export type ReleaseNotesResult =
  | { readonly kind: "ok"; readonly notes: string }
  | { readonly kind: "missing_version_section"; readonly version: PublicPackageVersion }
  | { readonly kind: "empty_version_section"; readonly version: PublicPackageVersion };

/**
 * Returns the notes under `version`'s changelog heading, formatted for a GitHub Release.
 *
 * GitHub renders every newline in a release body as a line break, so each hard-wrapped paragraph
 * or list item is joined back into one line.
 */
export function releaseNotes(args: {
  readonly changelog: string;
  readonly version: PublicPackageVersion;
}): ReleaseNotesResult {
  const lines = args.changelog.split("\n");
  const headingIndex = proseLines(lines).findIndex((line) =>
    line.startsWith(versionHeadingPrefix(args.version)),
  );
  if (headingIndex === -1) {
    return { kind: "missing_version_section", version: args.version };
  }
  const body = sectionBody(lines, headingIndex);
  if (!proseLines(body).some((line) => line.startsWith(LIST_ITEM_PREFIX))) {
    return { kind: "empty_version_section", version: args.version };
  }
  return { kind: "ok", notes: `${unwrap(body).join("\n").trim()}\n` };
}

/**
 * Joins each line that continues a paragraph or list item onto the line before it. Code blocks keep
 * their lines. This handles headings, paragraphs, `- ` lists and fenced code, which is what the
 * changelog uses.
 */
function unwrap(lines: readonly string[]): readonly string[] {
  const fenced = fencedLines(lines);
  return lines.reduce<string[]>((joined, line, index) => {
    const previous = joined.at(-1);
    if (
      previous === undefined ||
      fenced[index] === true ||
      fenced[index - 1] === true ||
      !continues({ previous, line })
    ) {
      return [...joined, line];
    }
    return [...joined.slice(0, -1), `${previous} ${line.trim()}`];
  }, []);
}

/** Whether `line` is wrapped text belonging to `previous` rather than a new block. */
function continues(args: { readonly previous: string; readonly line: string }): boolean {
  const text = args.line.trimStart();
  return (
    args.previous.trim() !== "" &&
    !args.previous.startsWith("#") &&
    text !== "" &&
    !text.startsWith("#") &&
    !text.startsWith(LIST_ITEM_PREFIX)
  );
}

function main(): number {
  const [versionArgument, ...extraArguments] = parseArgs({ allowPositionals: true }).positionals;
  const version = PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(versionArgument);
  if (!version.success || extraArguments.length > 0) {
    console.error("usage: release-notes.ts <semver>");
    return 1;
  }
  const result = releaseNotes({
    changelog: fs.readFileSync(CHANGELOG_URL, "utf8"),
    version: version.data,
  });
  if (result.kind !== "ok") {
    console.error(JSON.stringify(result));
    return 1;
  }
  process.stdout.write(result.notes);
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
