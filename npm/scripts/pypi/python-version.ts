import { parse as parseSemVer } from "semver";
import type { z } from "zod";
import type { PublicPackageVersion } from "../package-version.ts";

/** A release version spelled the way PyPI requires (PEP 440), such as `0.5.2b1`. */
export type PythonPackageVersion = string & z.$brand<"PythonPackageVersion">;

/**
 * How PEP 440 spells each SemVer prerelease label that Sprawl Guard releases use. A `dev` release
 * is a separate segment in PEP 440, so it takes a leading dot.
 */
const PRERELEASE_SPELLINGS = { alpha: "a", beta: "b", rc: "rc", dev: ".dev" } as const;

/** The result of converting a release version to its PyPI spelling. */
export type PythonPackageVersionResult =
  | { readonly kind: "ok"; readonly version: PythonPackageVersion }
  | { readonly kind: "unsupported_prerelease"; readonly releaseVersion: PublicPackageVersion };

/**
 * Converts a SemVer release version to PEP 440, as in `0.5.2-beta.1` → `0.5.2b1`.
 *
 * Only `<label>.<number>` prereleases with a label from {@link PRERELEASE_SPELLINGS} convert. Any
 * other prerelease fails instead of falling back to a looser PEP 440 reading, which would, for
 * example, turn `0.5.2-1` into the post-release `0.5.2.post1`.
 */
export function pythonPackageVersion(
  releaseVersion: PublicPackageVersion,
): PythonPackageVersionResult {
  const semVer = parseSemVer(releaseVersion);
  if (semVer === null) {
    throw new Error(`a validated release version did not parse as SemVer: ${releaseVersion}`);
  }
  const release = `${semVer.major}.${semVer.minor}.${semVer.patch}`;
  if (semVer.prerelease.length === 0) {
    return { kind: "ok", version: release as PythonPackageVersion };
  }
  const [label, number, ...rest] = semVer.prerelease;
  if (
    rest.length > 0 ||
    typeof number !== "number" ||
    typeof label !== "string" ||
    !Object.hasOwn(PRERELEASE_SPELLINGS, label)
  ) {
    return { kind: "unsupported_prerelease", releaseVersion };
  }
  const spelling = PRERELEASE_SPELLINGS[label as keyof typeof PRERELEASE_SPELLINGS];
  return { kind: "ok", version: `${release}${spelling}${number}` as PythonPackageVersion };
}
