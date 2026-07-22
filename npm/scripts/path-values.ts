import { resolveAbsolutePath, type AbsolutePath } from "../sprawl-guard/src/lib/paths.ts";

declare const absoluteDirectoryPathBrand: unique symbol;
declare const artifactRootDirectoryBrand: unique symbol;
declare const packageRelativePathBrand: unique symbol;
declare const payloadFileRelativePathBrand: unique symbol;
declare const publishWorkspaceRootDirectoryBrand: unique symbol;
declare const renderedWorkspaceRelativePathBrand: unique symbol;
declare const sourcePayloadRelativePathBrand: unique symbol;
declare const validationDetailBrand: unique symbol;

/** Absolute filesystem path that should point at a directory. */
export type AbsoluteDirectoryPath = AbsolutePath & {
  readonly [absoluteDirectoryPathBrand]: "AbsoluteDirectoryPath";
};

/** Absolute directory path containing one release artifact directory per runtime package. */
export type ArtifactRootDirectory = AbsoluteDirectoryPath & {
  readonly [artifactRootDirectoryBrand]: "ArtifactRootDirectory";
};

/** Absolute directory path of the npm workspace whose `.stage/` tree is being rendered. */
export type PublishWorkspaceRootDirectory = AbsoluteDirectoryPath & {
  readonly [publishWorkspaceRootDirectoryBrand]: "PublishWorkspaceRootDirectory";
};

/** Normalized package-relative path using POSIX separators. */
export type PackageRelativePath = string & {
  readonly [packageRelativePathBrand]: "PackageRelativePath";
};

/** Relative path expected under a payload validation root directory. */
export type PayloadFileRelativePath = PackageRelativePath & {
  readonly [payloadFileRelativePathBrand]: "PayloadFileRelativePath";
};

/** Relative path to a payload in a release artifact directory. */
export type SourcePayloadRelativePath = PackageRelativePath & {
  readonly [sourcePayloadRelativePathBrand]: "SourcePayloadRelativePath";
};

/** Relative path to a file under the rendered npm publish workspace. */
export type RenderedWorkspaceRelativePath = PackageRelativePath & {
  readonly [renderedWorkspaceRelativePathBrand]: "RenderedWorkspaceRelativePath";
};

/** Human-readable validation detail that is safe to display in CLI output. */
export type PayloadValidationDetail = string & {
  readonly [validationDetailBrand]: "PayloadValidationDetail";
};

/** Resolves a raw path string into an absolute directory path value. */
export function absoluteDirectoryPath(value: string): AbsoluteDirectoryPath {
  return resolveAbsolutePath(value) as unknown as AbsoluteDirectoryPath;
}

/** Resolves a raw path string into an artifact root directory path. */
export function artifactRootDirectory(value: string): ArtifactRootDirectory {
  return absoluteDirectoryPath(value) as unknown as ArtifactRootDirectory;
}

/** Resolves a raw path string into a publish workspace root directory path. */
export function publishWorkspaceRootDirectory(value: string): PublishWorkspaceRootDirectory {
  return absoluteDirectoryPath(value) as unknown as PublishWorkspaceRootDirectory;
}

/**
 * Brands a release artifact payload path.
 *
 * The input must already use normalized package-relative POSIX syntax. The function preserves
 * string literal types because it does not normalize the value.
 */
export function sourcePayloadRelativePath<const TValue extends string>(
  value: TValue,
): TValue & SourcePayloadRelativePath {
  assertPackageRelativePath(value);
  return value as unknown as TValue & SourcePayloadRelativePath;
}

/**
 * Brands a payload path relative to the package-like directory being validated.
 *
 * This is intentionally neutral: release artifact directories and staged npm package directories
 * both validate one payload file under a local root, but they are not the same domain.
 */
export function payloadFileRelativePath<const TValue extends string>(
  value: TValue,
): TValue & PayloadFileRelativePath {
  assertPackageRelativePath(value);
  return value as unknown as TValue & PayloadFileRelativePath;
}

/**
 * Brands a rendered publish workspace path.
 *
 * The input must already use normalized package-relative POSIX syntax. The function preserves
 * string literal types because it does not normalize the value.
 */
export function renderedWorkspaceRelativePath<const TValue extends string>(
  value: TValue,
): TValue & RenderedWorkspaceRelativePath {
  assertPackageRelativePath(value);
  return value as unknown as TValue & RenderedWorkspaceRelativePath;
}

/** Brands validation prose produced by these scripts. */
export function payloadValidationDetail<const TValue extends string>(
  value: TValue,
): TValue & PayloadValidationDetail {
  return value as unknown as TValue & PayloadValidationDetail;
}

/** Joins validation fragments into one display detail value. */
export function payloadValidationDetails(
  values: readonly (PackageRelativePath | PayloadValidationDetail)[],
): PayloadValidationDetail {
  return payloadValidationDetail(values.join(", "));
}

function assertPackageRelativePath(value: string): void {
  if (
    value.length === 0
    || value.includes("\0")
    || value.includes("\\")
    || value.startsWith("/")
    || /^[A-Za-z]:/.test(value)
  ) {
    throw new Error(`invalid package-relative path: ${value}`);
  }

  const segments = value.split("/");
  if (segments.some((segment) => segment === "" || segment === "." || segment === "..")) {
    throw new Error(`invalid package-relative path: ${value}`);
  }
}
