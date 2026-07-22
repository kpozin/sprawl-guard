import fs from "node:fs";
import type { AbsolutePath } from "../sprawl-guard/src/lib/paths.ts";

const FILE_NOT_FOUND_ERROR_CODE = "ENOENT" as const;

/** Result of inspecting one filesystem entry without following symbolic links. */
export type FilesystemEntryInspection =
  | {
      readonly kind: "present";
      readonly stats: fs.Stats;
    }
  | {
      readonly kind: "missing";
    }
  | {
      readonly kind: "filesystem_error";
      readonly cause: unknown;
    };

/**
 * Inspects one path while preserving the distinction between absence and operational failure.
 *
 * Only `ENOENT` means that the entry is absent. Permission failures, non-directory ancestors, and
 * other filesystem errors remain available to callers as typed operational failures.
 */
export function inspectFilesystemEntry(
  entryPath: AbsolutePath,
): FilesystemEntryInspection {
  try {
    return {
      kind: "present",
      stats: fs.lstatSync(entryPath),
    };
  } catch (cause) {
    if (isFileNotFoundError(cause)) {
      return { kind: "missing" };
    }

    return {
      kind: "filesystem_error",
      cause,
    };
  }
}

function isFileNotFoundError(cause: unknown): cause is NodeJS.ErrnoException {
  return cause instanceof Error
    && "code" in cause
    && cause.code === FILE_NOT_FOUND_ERROR_CODE;
}
