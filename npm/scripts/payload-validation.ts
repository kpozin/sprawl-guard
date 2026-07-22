import path from "node:path";
import { inspectFilesystemEntry } from "./filesystem-entry.ts";
import {
  PAYLOAD_CHECK_KIND,
  type PayloadCheckKind,
} from "./payload-check.ts";
import {
  type AbsoluteDirectoryPath,
  type PayloadFileRelativePath,
  type PayloadValidationDetail,
  payloadValidationDetail,
} from "./path-values.ts";
import { resolveAbsolutePath } from "../sprawl-guard/src/lib/paths.ts";

const OWNER_EXECUTABLE_MODE_BIT = 0o100 as const;

/** Result of validating payload files beneath an artifact or package directory. */
export type PayloadValidationResult =
  | {
      readonly kind: "checked";
      readonly missingPaths: readonly PayloadFileRelativePath[];
      readonly invalidPaths: readonly PayloadValidationDetail[];
    }
  | {
      readonly kind: "filesystem_error";
      readonly relativePath: PayloadFileRelativePath;
      readonly cause: unknown;
    };

/** Validates expected payload files beneath an artifact or package directory. */
export function validatePackagePayloads(args: {
  readonly packageDirectory: AbsoluteDirectoryPath;
  readonly payloadKind: PayloadCheckKind;
  readonly expectedPaths: readonly PayloadFileRelativePath[];
}): PayloadValidationResult {
  const missingPaths: PayloadFileRelativePath[] = [];
  const invalidPaths: PayloadValidationDetail[] = [];

  for (const expectedPath of args.expectedPaths) {
    const resolvedPath = resolveAbsolutePath(
      path.join(args.packageDirectory, expectedPath),
    );
    const inspection = inspectFilesystemEntry(resolvedPath);
    if (inspection.kind === "missing") {
      missingPaths.push(expectedPath);
      continue;
    }
    if (inspection.kind === "filesystem_error") {
      return {
        kind: "filesystem_error",
        relativePath: expectedPath,
        cause: inspection.cause,
      };
    }
    const stat = inspection.stats;

    if (stat.isSymbolicLink()) {
      invalidPaths.push(
        payloadValidationDetail(`${expectedPath} (symbolic links are not allowed)`),
      );
      continue;
    }

    if (!stat.isFile()) {
      invalidPaths.push(
        payloadValidationDetail(`${expectedPath} (expected a regular file)`),
      );
      continue;
    }

    if (
      args.payloadKind === PAYLOAD_CHECK_KIND.POSIX_EXECUTABLE
      && process.platform === "win32"
    ) {
      invalidPaths.push(
        payloadValidationDetail(
          `${expectedPath} (cannot validate executable bit on Windows)`,
        ),
      );
      continue;
    }

    if (
      args.payloadKind === PAYLOAD_CHECK_KIND.POSIX_EXECUTABLE
      && (stat.mode & OWNER_EXECUTABLE_MODE_BIT) === 0
    ) {
      invalidPaths.push(
        payloadValidationDetail(`${expectedPath} (missing executable bit)`),
      );
    }
  }

  return {
    kind: "checked",
    missingPaths,
    invalidPaths,
  };
}
