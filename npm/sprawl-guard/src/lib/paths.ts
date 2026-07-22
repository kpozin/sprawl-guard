import path from "node:path";

declare const pathBrand: unique symbol;
declare const pathKindBrand: unique symbol;
declare const pathRoleBrand: unique symbol;

type PathKind<TKind extends string> = {
  readonly [pathKindBrand]: TKind;
};

type PathRole<TRole extends string> = {
  readonly [pathRoleBrand]: TRole;
};

/** Branded filesystem path string. */
export type Path = string & {
  readonly [pathBrand]: "Path";
};

/** Absolute filesystem path. */
export type AbsolutePath = Path & PathKind<"AbsolutePath">;

/** Absolute path to an executable file. */
export type ExecutablePath = AbsolutePath & PathRole<"ExecutablePath">;
/** Absolute repository root path exposed to the WASI module. */
export type HostRootPath = AbsolutePath & PathRole<"HostRootPath">;
/** Absolute package root directory path. */
export type PackageRootPath = AbsolutePath & PathRole<"PackageRootPath">;
/** Absolute path to the fallback WASM module file. */
export type WasmModulePath = AbsolutePath & PathRole<"WasmModulePath">;

function brandNormalizedPath<TPath extends Path>(value: string): TPath {
  return value as unknown as TPath;
}

function brandExistingPath<TPath extends Path, TValue extends string>(
  value: TValue,
): TValue & TPath {
  return value as unknown as TValue & TPath;
}

/** Resolves an arbitrary filesystem path to an absolute path. */
export function resolveAbsolutePath(value: string): AbsolutePath {
  return brandNormalizedPath<AbsolutePath>(path.resolve(value));
}

/**
 * Adds the absolute-path brand to a string that is already trusted to be absolute.
 *
 * This helper does not validate or normalize its input.
 */
export function unsafeAbsolutePathFromKnownAbsolute<TValue extends string>(
  value: TValue,
): TValue & AbsolutePath {
  return brandExistingPath<AbsolutePath, TValue>(value);
}

/**
 * Adds the executable-file role to an already trusted absolute path.
 *
 * This helper does not validate that the path points to an executable file.
 */
export function unsafeExecutablePathFromAbsolutePath<TValue extends AbsolutePath>(
  value: TValue,
): TValue & ExecutablePath {
  return brandExistingPath<ExecutablePath, TValue>(value);
}

/**
 * Adds the WASI host-root role to an already trusted absolute path.
 *
 * This helper does not validate repository semantics beyond the existing absolute-path brand.
 */
export function unsafeHostRootPathFromAbsolutePath<TValue extends AbsolutePath>(
  value: TValue,
): TValue & HostRootPath {
  return brandExistingPath<HostRootPath, TValue>(value);
}

/** Resolves a package root from the absolute path to its `package.json` file. */
export function packageRootPathFromPackageJsonPath(
  packageJsonPath: AbsolutePath,
): PackageRootPath {
  return brandNormalizedPath<PackageRootPath>(path.dirname(packageJsonPath));
}

/** Resolves the native binary path beneath a trusted package root. */
export function resolveExecutablePath(
  packageRoot: PackageRootPath,
  platform: NodeJS.Platform = process.platform,
): ExecutablePath {
  const binaryName = platform === "win32" ? "sprawl-guard.exe" : "sprawl-guard";
  return brandNormalizedPath<ExecutablePath>(path.join(packageRoot, "bin", binaryName));
}

/** Resolves the fallback WASM module path beneath a trusted package root. */
export function resolveWasmModulePath(packageRoot: PackageRootPath): WasmModulePath {
  return brandNormalizedPath<WasmModulePath>(path.join(packageRoot, "sprawl-guard.wasm"));
}
