import {
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  type PublicPackageVersion,
} from "../package-version.ts";

/** The start of a heading for a release or the `[Unreleased]` section. */
const SECTION_HEADING_PREFIX = "## ";
/** A section heading such as `## [0.5.0] - 2026-11-17`, capturing the bracketed name. */
const SECTION_NAME_PATTERN = new RegExp(`^${SECTION_HEADING_PREFIX}\\[([^\\]]+)\\]`, "u");
/** The start of a changelog entry; a section without one has no notes, only headings. */
export const LIST_ITEM_PREFIX = "- ";
/** A code fence: a run of three or more backticks or tildes, capturing the run and what follows. */
const CODE_FENCE_PATTERN = /^\s*(`{3,}|~{3,})(.*)$/u;

/**
 * Marks each line that is a code fence or lies between an opening and a closing fence. As in
 * CommonMark, only a bare run of the opener's character, at least as long, closes the block, so an
 * example of a shorter fence stays inside it.
 */
export function fencedLines(lines: readonly string[]): readonly boolean[] {
  let opener: string | undefined;
  return lines.map((line) => {
    const [, run, rest] = CODE_FENCE_PATTERN.exec(line) ?? [];
    if (opener === undefined) {
      opener = run;
      return run !== undefined;
    }
    // A run of one character that starts with the opener is the same character and at least as long.
    if (run?.startsWith(opener) === true && rest?.trim() === "") {
      opener = undefined;
    }
    return true;
  });
}

/**
 * Returns `lines` with every fenced line blank, keeping indexes, so a heading or list item inside a
 * code example isn't read as part of the changelog's structure.
 */
export function proseLines(lines: readonly string[]): readonly string[] {
  const fenced = fencedLines(lines);
  return lines.map((line, index) => (fenced[index] === true ? "" : line));
}

/**
 * Returns the start of `version`'s heading. The closing bracket keeps `0.5.1` from matching
 * `0.5.10`.
 */
export function versionHeadingPrefix(version: PublicPackageVersion): string {
  return `${SECTION_HEADING_PREFIX}[${version}]`;
}

/** Returns the version of every release heading, newest first, as the changelog lists them. */
export function releases(lines: readonly string[]): readonly PublicPackageVersion[] {
  return proseLines(lines).flatMap((line) => {
    const version = PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(SECTION_NAME_PATTERN.exec(line)?.[1]);
    return version.success ? [version.data] : [];
  });
}

/**
 * Returns the lines between the heading at `headingIndex` and the next section's heading outside a
 * code block.
 */
export function sectionBody(lines: readonly string[], headingIndex: number): readonly string[] {
  const nextSectionIndex = proseLines(lines).findIndex(
    (line, index) => index > headingIndex && line.startsWith(SECTION_HEADING_PREFIX),
  );
  return lines.slice(headingIndex + 1, nextSectionIndex === -1 ? undefined : nextSectionIndex);
}
