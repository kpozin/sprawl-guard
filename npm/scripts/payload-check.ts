/** Payload checks supported by release artifact and staged-package validation. */
export const PAYLOAD_CHECK_KIND = {
  FILE: "file",
  POSIX_EXECUTABLE: "posix-executable",
} as const;

/** One payload check selected by runtime-package build metadata. */
export type PayloadCheckKind =
  (typeof PAYLOAD_CHECK_KIND)[keyof typeof PAYLOAD_CHECK_KIND];
