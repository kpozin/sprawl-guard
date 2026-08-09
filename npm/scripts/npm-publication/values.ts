import path from "node:path";
import { z } from "zod";
import {
  ABSOLUTE_DIRECTORY_PATH_SCHEMA,
  PACKAGE_RELATIVE_PATH_SCHEMA,
  type AbsoluteDirectoryPath,
  type PackageRelativePath,
} from "../path-values.ts";
import {
  RUNTIME_PACKAGE_DEFINITIONS,
  WRAPPER_PACKAGE_NAME,
} from "../package-manifests.ts";

const NPM_PACKAGE_NAMES = new Set<string>([
  ...RUNTIME_PACKAGE_DEFINITIONS.map(({ packageName }) => packageName),
  WRAPPER_PACKAGE_NAME,
]);
const NPM_PACKAGE_INTEGRITY_PATTERN =
  /^sha512-[A-Za-z0-9+/]{86}==$/u;

/** Public npm registry used for every package read and mutation. */
export const NPM_REGISTRY_URL = "https://registry.npmjs.org/";

/** Name of one npm package in the sprawl-guard release set. */
export type NpmPackageName =
  | (typeof RUNTIME_PACKAGE_DEFINITIONS)[number]["packageName"]
  | typeof WRAPPER_PACKAGE_NAME;

/** Validated package name belonging to the complete sprawl-guard npm release. */
export const NPM_PACKAGE_NAME_SCHEMA = z.custom<NpmPackageName>(
  (value) =>
    typeof value === "string"
    && NPM_PACKAGE_NAMES.has(value),
  "must name one sprawl-guard npm package",
);

export {
  ABSOLUTE_DIRECTORY_PATH_SCHEMA,
  PACKAGE_RELATIVE_PATH_SCHEMA,
};
export type { AbsoluteDirectoryPath, PackageRelativePath };

/** Parses a complete list of normalized package-relative paths. */
export function parsePackageRelativePaths(
  values: readonly string[],
): readonly PackageRelativePath[] {
  return PACKAGE_RELATIVE_PATH_SCHEMA.array().parse(values);
}

/** Basename of one immutable npm package archive. */
export const NPM_ARCHIVE_FILE_NAME_SCHEMA = z
  .string()
  .refine(
    (value) =>
      value.endsWith(".tgz")
      && path.posix.basename(value) === value
      && path.win32.basename(value) === value,
    "must be a .tgz archive basename",
  )
  .brand<"NpmArchiveFileName">();

/** Basename of one immutable npm package archive. */
export type NpmArchiveFileName = z.infer<typeof NPM_ARCHIVE_FILE_NAME_SCHEMA>;

/** Absolute path of one immutable npm package archive. */
export const NPM_ARCHIVE_PATH_SCHEMA = z
  .string()
  .refine(
    (value) => path.isAbsolute(value) && value.endsWith(".tgz"),
    "must be an absolute .tgz archive path",
  )
  .brand<"NpmArchivePath">();

/** Absolute path of one immutable npm package archive. */
export type NpmArchivePath = z.infer<typeof NPM_ARCHIVE_PATH_SCHEMA>;

/** SHA-512 SRI value that identifies an immutable npm package archive. */
export const NPM_PACKAGE_INTEGRITY_SCHEMA = z
  .string()
  .regex(NPM_PACKAGE_INTEGRITY_PATTERN)
  .brand<"NpmPackageIntegrity">();

/** SHA-512 SRI value that identifies an immutable npm package archive. */
export type NpmPackageIntegrity = z.infer<
  typeof NPM_PACKAGE_INTEGRITY_SCHEMA
>;
