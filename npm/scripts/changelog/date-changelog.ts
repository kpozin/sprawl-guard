#!/usr/bin/env node

import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { gt as semverGt } from "semver";
import { z } from "zod";
import {
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  type PublicPackageVersion,
} from "../package-version.ts";
import {
  latestRelease,
  LIST_ITEM_PREFIX,
  proseLines,
  sectionBody,
  versionHeadingPrefix,
} from "./sections.ts";

/** The changelog at the public repository root. */
const CHANGELOG_URL = new URL("../../../CHANGELOG.md", import.meta.url);
/** The heading of the section that collects changes for the next release. */
const UNRELEASED_HEADING = "## [Unreleased]";
/** A release date as `YYYY-MM-DD`. */
export const RELEASE_DATE_SCHEMA = z.iso.date().brand<"ReleaseDate">();

/** The date a release's changelog section records. */
export type ReleaseDate = z.infer<typeof RELEASE_DATE_SCHEMA>;

/** Result of moving the unreleased changes into a dated release section. */
export type DateChangelogResult =
  | { readonly kind: "ok"; readonly changelog: string }
  | { readonly kind: "missing_unreleased_section" }
  | { readonly kind: "empty_unreleased_section" }
  | {
      readonly kind: "version_not_newer";
      readonly version: PublicPackageVersion;
      readonly latest: PublicPackageVersion;
    };

/**
 * Turns the `[Unreleased]` section into the section for `version`, released on `date`, and opens an
 * empty `[Unreleased]` section above it. `version` must come after the latest release, so
 * preparing it can't repeat or downgrade a release.
 */
export function dateChangelog(args: {
  readonly changelog: string;
  readonly version: PublicPackageVersion;
  readonly date: ReleaseDate;
}): DateChangelogResult {
  const lines = args.changelog.split("\n");
  const unreleasedIndex = proseLines(lines).indexOf(UNRELEASED_HEADING);
  if (unreleasedIndex === -1) {
    return { kind: "missing_unreleased_section" };
  }
  const latest = latestRelease(lines.slice(unreleasedIndex + 1));
  if (latest !== undefined && !semverGt(args.version, latest)) {
    return { kind: "version_not_newer", version: args.version, latest };
  }
  const unreleasedBody = proseLines(sectionBody(lines, unreleasedIndex));
  if (!unreleasedBody.some((line) => line.startsWith(LIST_ITEM_PREFIX))) {
    return { kind: "empty_unreleased_section" };
  }
  return {
    kind: "ok",
    changelog: lines
      .toSpliced(unreleasedIndex + 1, 0, "", `${versionHeadingPrefix(args.version)} - ${args.date}`)
      .join("\n"),
  };
}

/** Writes the dated changelog, or only reports whether it could be dated. */
const MODE_SCHEMA = z.enum(["--check", "--write"]);

function main(): number {
  const [modeArgument, versionArgument, dateArgument, ...extraArguments] = process.argv.slice(2);
  const mode = MODE_SCHEMA.safeParse(modeArgument);
  const version = PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(versionArgument);
  const date = RELEASE_DATE_SCHEMA.safeParse(dateArgument);
  if (!mode.success || !version.success || !date.success || extraArguments.length > 0) {
    console.error("usage: date-changelog.ts --check|--write <semver> <YYYY-MM-DD>");
    return 1;
  }
  const result = dateChangelog({
    changelog: fs.readFileSync(CHANGELOG_URL, "utf8"),
    version: version.data,
    date: date.data,
  });
  if (result.kind !== "ok") {
    console.error(JSON.stringify(result));
    return 1;
  }
  if (mode.data === "--write") {
    fs.writeFileSync(CHANGELOG_URL, result.changelog);
  }
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
