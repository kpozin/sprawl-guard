import { createRequire } from "node:module";
import { Result } from "better-result";
import { RuntimeFailureError } from "./errors.js";
import {
  unsafeAbsolutePathFromKnownAbsolute,
  packageRootPathFromPackageJsonPath,
  resolveExecutablePath,
  resolveWasmModulePath as resolvePackageWasmModulePath,
  type ExecutablePath,
  type PackageRootPath,
  type WasmModulePath,
} from "./paths.js";
import {
  NATIVE_RUNTIME_PACKAGE_DEFINITIONS,
  WASM_PACKAGE_NAME,
  type LibcFamily,
  type NativePackageName,
  type RuntimePackageName,
} from "./runtime-packages.js";

const require = createRequire(import.meta.url);

export { WASM_PACKAGE_NAME };
export type WasmPackageName = typeof WASM_PACKAGE_NAME;
export type { LibcFamily, NativePackageName, RuntimePackageName };

/** Runtime dimensions that select a native binary distribution package. */
export type NativeRuntime =
  | {
      platform: "linux";
      arch: NodeJS.Architecture;
      libcFamily: LibcFamily;
    }
  | {
      platform: Exclude<NodeJS.Platform, "linux">;
      arch: NodeJS.Architecture;
    };

/** Detects whether the current Linux runtime uses glibc or musl. */
export function detectLibcFamily(): LibcFamily | null {
  if (process.platform !== "linux") {
    return null;
  }

  const report = process.report?.getReport?.() as
    | {
        header?: {
          glibcVersionRuntime?: string;
        };
      }
    | undefined;
  return report?.header?.glibcVersionRuntime ? "gnu" : "musl";
}

/** Returns the current runtime dimensions used to select an optional native package. */
export function currentNativeRuntime(): NativeRuntime {
  const platform = process.platform;
  if (platform === "linux") {
    return {
      platform,
      arch: process.arch,
      libcFamily: detectLibcFamily() ?? "musl",
    };
  }

  return {
    platform,
    arch: process.arch,
  };
}

/** Resolves the native distribution package for the current Node platform and architecture. */
export function resolveNativePackageName(
  runtime: NativeRuntime = currentNativeRuntime(),
): NativePackageName | null {
  for (const definition of NATIVE_RUNTIME_PACKAGE_DEFINITIONS) {
    if (definition.platform !== runtime.platform || definition.arch !== runtime.arch) {
      continue;
    }
    if (
      runtime.platform === "linux"
      && (!("libcFamily" in definition) || definition.libcFamily !== runtime.libcFamily)
    ) {
      continue;
    }
    return definition.packageName;
  }
  return null;
}

/** Resolves the installed root directory for a native or WASI distribution package. */
export function resolveInstalledPackageRoot(
  packageName: RuntimePackageName,
): Result<PackageRootPath | null, RuntimeFailureError> {
  const resolution = Result.try({
    try: () => require.resolve(`${packageName}/package.json`),
    catch: (cause) =>
      new RuntimeFailureError(
        "resolve package root",
        `failed to resolve package ${packageName}`,
        cause,
      ),
  });

  if (Result.isOk(resolution)) {
    return Result.ok(
      packageRootPathFromPackageJsonPath(
        unsafeAbsolutePathFromKnownAbsolute(resolution.value),
      ),
    );
  }

  const nodeError = resolution.error.cause as NodeJS.ErrnoException | undefined;
  if (nodeError?.code === "MODULE_NOT_FOUND") {
    return Result.ok(null);
  }

  return Result.err(resolution.error);
}

/** Resolves the native binary path inside a trusted package root. */
export function resolveNativeBinaryPath(
  packageRoot: PackageRootPath,
  platform: NodeJS.Platform = process.platform,
): ExecutablePath {
  return resolveExecutablePath(packageRoot, platform);
}

/** Resolves the fallback WASM module path inside a trusted package root. */
export function resolveWasmModulePath(packageRoot: PackageRootPath): WasmModulePath {
  return resolvePackageWasmModulePath(packageRoot);
}
