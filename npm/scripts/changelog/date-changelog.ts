#!/usr/bin/env node

import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { prerelease, gt as semverGt } from "semver";
import { z } from "zod";
import {
  loadPublicPackageVersion,
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  type PublicPackageVersion,
} from "../package-version.ts";
import {
  LIST_ITEM_PREFIX,
  proseLines,
  releases,
  sectionBody,
  versionHeadingPrefix,
} from "./sections.ts";

/** The changelog at the public repository root. */
const CHANGELOG_URL = new URL("../../../CHANGELOG.md", import.meta.url);
/** The file that holds the version every public package shares. */
const PACKAGE_VERSION_PATH = fileURLToPath(new URL("../../package-version.json", import.meta.url));
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
    }
  | {
      /**
       * The version isn't after the current package version, which may belong to a prerelease
       * whose changelog section was folded away before a stable release.
       */
      readonly kind: "version_not_after_package";
      readonly version: PublicPackageVersion;
      readonly packageVersion: PublicPackageVersion;
    }
  | {
      /** A stable release would ship notes for these prereleases. */
      readonly kind: "prerelease_sections_present";
      readonly prereleases: readonly PublicPackageVersion[];
    };

/** Where release preparation stands relative to setting the package version. */
export type ReleaseStage =
  /** The package still has its current version, which the new version must come after. */
  | { readonly kind: "before_set_version"; readonly packageVersion: PublicPackageVersion }
  /** The package already has the new version. */
  | { readonly kind: "after_set_version" };

/**
 * Turns the `[Unreleased]` section into the section for `version`, released on `date`, and opens an
 * empty `[Unreleased]` section above it. `version` must come after the latest release, so
 * preparing it can't repeat or downgrade a release.
 *
 * A stable release's changelog has no prerelease sections. Before preparing one, the owner rewrites
 * the prereleases' notes under `[Unreleased]` as changes since the last stable release. Because that
 * removes their headings, `version` must also come after the package version before it is set.
 */
export function dateChangelog(args: {
  readonly changelog: string;
  readonly version: PublicPackageVersion;
  readonly stage: ReleaseStage;
  readonly date: ReleaseDate;
}): DateChangelogResult {
  const lines = args.changelog.split("\n");
  const unreleasedIndex = proseLines(lines).indexOf(UNRELEASED_HEADING);
  if (unreleasedIndex === -1) {
    return { kind: "missing_unreleased_section" };
  }
  const released = releases(lines.slice(unreleasedIndex + 1));
  const [latest] = released;
  if (latest !== undefined && !semverGt(args.version, latest)) {
    return { kind: "version_not_newer", version: args.version, latest };
  }
  if (
    args.stage.kind === "before_set_version" &&
    !semverGt(args.version, args.stage.packageVersion)
  ) {
    return {
      kind: "version_not_after_package",
      version: args.version,
      packageVersion: args.stage.packageVersion,
    };
  }
  const prereleases = released.filter((version) => prerelease(version) !== null);
  if (prerelease(args.version) === null && prereleases.length > 0) {
    return { kind: "prerelease_sections_present", prereleases };
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
function main(): number {
  const { values, positionals } = parseArgs({
    options: { check: { type: "boolean" }, write: { type: "boolean" } },
    allowPositionals: true,
  });
  const [versionArgument, dateArgument, ...extraArguments] = positionals;
  const version = PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(versionArgument);
  const date = RELEASE_DATE_SCHEMA.safeParse(dateArgument);
  if (
    values.check === values.write ||
    !version.success ||
    !date.success ||
    extraArguments.length > 0
  ) {
    console.error("usage: date-changelog.ts --check|--write <semver> <YYYY-MM-DD>");
    return 1;
  }
  const result = dateChangelog({
    changelog: fs.readFileSync(CHANGELOG_URL, "utf8"),
    version: version.data,
    // The recipe runs `--check` before it sets the version and `--write` after.
    stage: values.check
      ? { kind: "before_set_version", packageVersion: loadPublicPackageVersion(PACKAGE_VERSION_PATH) }
      : { kind: "after_set_version" },
    date: date.data,
  });
  if (result.kind !== "ok") {
    console.error(JSON.stringify(result));
    return 1;
  }
  if (values.write) {
    fs.writeFileSync(CHANGELOG_URL, result.changelog);
  }
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
