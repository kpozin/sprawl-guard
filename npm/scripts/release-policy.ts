import { z } from "zod";
import { prerelease } from "semver";
import type { NpmDistTag } from "./npm-dist-tag.ts";
import { RUNTIME_PACKAGE_DEFINITIONS } from "./package-manifests.ts";
import {
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  type PublicPackageVersion,
} from "./package-version.ts";
/** Validates a `v`-prefixed tag containing one publishable package version. */
export const RELEASE_TAG_SCHEMA = z
  .string()
  .regex(/^v/)
  .transform((tag, context) => {
    const version = PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(tag.slice(1));
    if (!version.success) {
      context.addIssue({
        code: "custom",
        message:
          "release tag must contain a publishable version after its v prefix",
      });
      return z.NEVER;
    }
    return tag as `v${ReleaseVersion}`;
  })
  .brand<"ReleaseTag">();

const RELEASED_CARGO_PACKAGE_NAMES = [
  "sprawl-guard",
  "sprawl-guard-lib",
] as const;
const NPM_WRAPPER_PACKAGE_NAME = "@kpozin/sprawl-guard" as const;

/** A canonical SemVer version represented identically by Cargo and npm. */
export type ReleaseVersion = PublicPackageVersion;

/** A `v`-prefixed release tag containing a valid release version. */
export type ReleaseTag = z.infer<typeof RELEASE_TAG_SCHEMA>;

/** The fully qualified Git ref for a validated release tag. */
export type ReleaseRef = `refs/tags/${ReleaseTag}`;

/** A file name that belongs to the complete GitHub Release asset set. */
export type ReleaseAssetName = string & z.BRAND<"ReleaseAssetName">;

/** Result of parsing an untrusted Git tag as public release metadata. */
export type ParseReleaseTagResult =
  | {
      readonly kind: "ok";
      readonly releaseRef: ReleaseRef;
      readonly releaseTag: ReleaseTag;
      readonly releaseVersion: ReleaseVersion;
    }
  | {
      readonly kind: "invalid_release_tag";
      readonly releaseTag: string;
    };

/** One Cargo package name and version from `cargo metadata`. */
export interface CargoPackageVersion {
  readonly name: string;
  readonly version: PublicPackageVersion;
}

/** A package whose version prevents the requested release. */
export interface ReleaseVersionMismatch {
  readonly actualVersion: PublicPackageVersion | null;
  readonly packageName: string;
}

/** Result of checking every public package against a release version. */
export type ValidateReleaseVersionsResult =
  | { readonly kind: "ok" }
  | {
      readonly kind: "version_mismatch";
      readonly mismatches: readonly ReleaseVersionMismatch[];
      readonly releaseVersion: ReleaseVersion;
    };

/** GitHub Release state derived from the SemVer and selected npm channel. */
export type GitHubReleaseDisposition =
  | { readonly kind: "prerelease" }
  | { readonly kind: "stable_latest" }
  | { readonly kind: "stable_non_latest" };

/** Parses a tag containing a canonical version supported by both registries. */
export function parseReleaseTag(releaseTag: string): ParseReleaseTagResult {
  const parsedTag = RELEASE_TAG_SCHEMA.safeParse(releaseTag);
  if (!parsedTag.success) {
    return {
      kind: "invalid_release_tag",
      releaseTag,
    };
  }

  const releaseVersion = PUBLIC_PACKAGE_VERSION_SCHEMA.parse(
    parsedTag.data.slice(1),
  );
  return {
    kind: "ok",
    releaseRef: `refs/tags/${parsedTag.data}`,
    releaseTag: parsedTag.data,
    releaseVersion,
  };
}

/** Verifies that the Rust crates and npm packages all carry the release version. */
export function validateReleaseVersions(args: {
  readonly cargoPackages: readonly CargoPackageVersion[];
  readonly npmVersion: PublicPackageVersion;
  readonly releaseVersion: ReleaseVersion;
}): ValidateReleaseVersionsResult {
  const cargoVersions = new Map(
    args.cargoPackages.map((cargoPackage) => [
      cargoPackage.name,
      cargoPackage.version,
    ]),
  );
  const mismatches: ReleaseVersionMismatch[] = [];

  for (const packageName of RELEASED_CARGO_PACKAGE_NAMES) {
    const actualVersion = cargoVersions.get(packageName) ?? null;
    if (actualVersion !== args.releaseVersion) {
      mismatches.push({ actualVersion, packageName });
    }
  }

  if (args.npmVersion !== args.releaseVersion) {
    mismatches.push({
      actualVersion: args.npmVersion,
      packageName: NPM_WRAPPER_PACKAGE_NAME,
    });
  }

  return mismatches.length === 0
    ? { kind: "ok" }
    : {
        kind: "version_mismatch",
        mismatches,
        releaseVersion: args.releaseVersion,
      };
}

/** Classifies a GitHub release from its SemVer version and npm channel. */
export function classifyGitHubRelease(
  releaseVersion: ReleaseVersion,
  npmDistTag: NpmDistTag,
): GitHubReleaseDisposition {
  if (prerelease(releaseVersion) !== null) {
    return { kind: "prerelease" };
  }
  return npmDistTag === "latest"
    ? { kind: "stable_latest" }
    : { kind: "stable_non_latest" };
}

/** Derives the complete release asset set from shared runtime-target metadata. */
export function expectedReleaseAssetNames(
  releaseVersion: ReleaseVersion,
): readonly ReleaseAssetName[] {
  const runtimeAssets = RUNTIME_PACKAGE_DEFINITIONS.map((definition) => {
    const extension = definition.slug === "win32-x64-msvc" ? "zip" : "tar.gz";
    return `sprawl-guard-${releaseVersion}-${definition.rustTarget}.${extension}` as ReleaseAssetName;
  });
  return [
    ...runtimeAssets,
    `sprawl-guard-${releaseVersion}-checksums.txt` as ReleaseAssetName,
  ];
}
