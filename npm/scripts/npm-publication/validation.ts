import { createHash } from "node:crypto";
import fs from "node:fs";
import type { ValidationResult } from "./model.ts";
import {
  NPM_PACKAGE_INTEGRITY_SCHEMA,
  type NpmArchivePath,
  type NpmPackageIntegrity,
} from "./values.ts";

const EXPECTED_PNPM_VERSION = "11.7.0";

/** Computes the SHA-512 SRI value for an immutable npm package archive. */
export function archiveIntegrity(
  archivePath: NpmArchivePath,
): NpmPackageIntegrity {
  return NPM_PACKAGE_INTEGRITY_SCHEMA.parse(
    `sha512-${createHash("sha512").update(fs.readFileSync(archivePath)).digest("base64")}`,
  );
}

/** Verifies that an archive still has the integrity recorded in its candidate manifest. */
export function validateArchiveIntegrity(
  archivePath: NpmArchivePath,
  expectedIntegrity: NpmPackageIntegrity,
): ValidationResult {
  try {
    const actualIntegrity = archiveIntegrity(archivePath);
    return actualIntegrity === expectedIntegrity
      ? { kind: "valid" }
      : {
          kind: "invalid",
          detail: `archive integrity changed: expected ${expectedIntegrity}; received ${actualIntegrity}`,
        };
  } catch (cause) {
    return {
      kind: "invalid",
      detail: `archive integrity could not be verified: ${String(cause)}`,
    };
  }
}

/** Validates that a candidate path names one independent regular file. */
export function validateRegularCandidateFile(filePath: string): ValidationResult {
  try {
    const status = fs.lstatSync(filePath);
    return status.isFile() && status.nlink === 1
      ? { kind: "valid" }
      : {
          kind: "invalid",
          detail: `npm candidate entry is not an independent regular file: ${filePath}`,
        };
  } catch (cause) {
    return {
      kind: "invalid",
      detail: `failed to inspect npm candidate entry ${filePath}: ${String(cause)}`,
    };
  }
}

/** Validates the audited pnpm version used to form immutable package archives. */
export function validatePnpmVersion(actualVersion: string): ValidationResult {
  return actualVersion === EXPECTED_PNPM_VERSION
    ? { kind: "valid" }
    : {
        kind: "invalid",
        detail: `pnpm ${EXPECTED_PNPM_VERSION} is required; found ${actualVersion}`,
      };
}

/** Validates the exact generated manifest before pnpm can run package scripts. */
export function validateStagedPackageManifest(args: {
  readonly actualText: string;
  readonly expectedText: string;
}): ValidationResult {
  return args.actualText === args.expectedText
    ? { kind: "valid" }
    : {
        kind: "invalid",
        detail: "staged package.json differs from the generated release manifest",
      };
}

/** Validates the semantic JSON in a manifest read from a package archive. */
export function validatePackedPackageManifest(args: {
  readonly actualText: string;
  readonly expectedText: string;
}): ValidationResult {
  try {
    return JSON.stringify(JSON.parse(args.actualText))
      === JSON.stringify(JSON.parse(args.expectedText))
      ? { kind: "valid" }
      : {
          kind: "invalid",
          detail: "packed package.json differs from the generated release manifest",
        };
  } catch (cause) {
    return {
      kind: "invalid",
      detail: `packed package.json is invalid: ${String(cause)}`,
    };
  }
}

/** Validates that two path sets contain exactly the same entries. */
export function validatePackedPaths(args: {
  readonly actualPaths: readonly string[];
  readonly expectedPaths: readonly string[];
}): ValidationResult {
  const actual = args.actualPaths.toSorted();
  const expected = args.expectedPaths.toSorted();
  return actual.length === expected.length
    && actual.every((actualPath, index) => actualPath === expected[index])
    ? { kind: "valid" }
    : {
        kind: "invalid",
        detail: `packed paths differ: expected ${expected.join(", ")}; received ${actual.join(", ")}`,
      };
}
