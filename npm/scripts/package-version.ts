import fs from "node:fs";
import { valid as validSemVer } from "semver";
import { z } from "zod";

/** Schema shared by Cargo and npm for exact, consistently representable versions. */
export const PUBLIC_PACKAGE_VERSION_SCHEMA = z
  .string()
  .refine(
    (value) => !value.includes("+") && validSemVer(value) === value,
    "version must be canonical npm SemVer without build metadata",
  )
  .brand<"PublicPackageVersion">();

/** Version shared by a public npm package, Cargo crate, and release tag. */
export type PublicPackageVersion = z.infer<
  typeof PUBLIC_PACKAGE_VERSION_SCHEMA
>;

/** Kind of failure encountered while loading canonical package-version metadata. */
export type PublicPackageVersionFileErrorKind =
  "read_failed" | "invalid_json" | "invalid_schema";

/** Typed failure to read or validate canonical package-version metadata. */
export class PublicPackageVersionFileError extends Error {
  readonly kind: PublicPackageVersionFileErrorKind;
  readonly versionPath: string;

  constructor(args: {
    readonly cause: unknown;
    readonly kind: PublicPackageVersionFileErrorKind;
    readonly versionPath: string;
  }) {
    super(
      `invalid public package version file ${args.versionPath}: ${String(args.cause)}`,
      {
        cause: args.cause,
      },
    );
    this.name = "PublicPackageVersionFileError";
    this.kind = args.kind;
    this.versionPath = args.versionPath;
  }
}

const PUBLIC_PACKAGE_VERSION_FILE_SCHEMA = z
  .object({ version: PUBLIC_PACKAGE_VERSION_SCHEMA })
  .strict();

/** Loads the canonical public package version from a strict JSON file. */
export function loadPublicPackageVersion(
  versionPath: string,
): PublicPackageVersion {
  let text: string;
  try {
    text = fs.readFileSync(versionPath, "utf8");
  } catch (cause) {
    throw new PublicPackageVersionFileError({
      cause,
      kind: "read_failed",
      versionPath,
    });
  }
  let value: unknown;
  try {
    value = JSON.parse(text) as unknown;
  } catch (cause) {
    throw new PublicPackageVersionFileError({
      cause,
      kind: "invalid_json",
      versionPath,
    });
  }
  const parsed = PUBLIC_PACKAGE_VERSION_FILE_SCHEMA.safeParse(value);
  if (!parsed.success) {
    throw new PublicPackageVersionFileError({
      cause: parsed.error,
      kind: "invalid_schema",
      versionPath,
    });
  }
  return parsed.data.version;
}

/**
 * Brands a trusted package-version literal after validating it.
 *
 * This preserves the literal type because validation does not normalize the value. A failure is a
 * programming error in a checked-in constant rather than an expected runtime input error.
 */
export function trustedPublicPackageVersion<const TValue extends string>(
  value: TValue,
): TValue & PublicPackageVersion {
  const parsed = PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(value);
  if (!parsed.success) {
    throw new Error(`invalid public package version: ${value}`);
  }
  return value as TValue & PublicPackageVersion;
}
