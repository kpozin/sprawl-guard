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
  GLIBC_FLOOR,
  NATIVE_RUNTIME_PACKAGE_DEFINITIONS,
  WASM_PACKAGE_NAME,
  type GlibcVersion,
  type LibcFamily,
  type NativePackageName,
  type RuntimePackageName,
} from "./runtime-packages.js";

const require = createRequire(import.meta.url);

export { WASM_PACKAGE_NAME };
export type WasmPackageName = typeof WASM_PACKAGE_NAME;
export type { LibcFamily, NativePackageName, RuntimePackageName };

/** The C library of a Linux system. A glibc binary also needs a new enough glibc to load. */
export type LinuxLibc =
  | { readonly family: "musl" }
  | { readonly family: "gnu"; readonly version: GlibcVersion };

/** Runtime dimensions that select a native binary distribution package. */
export type NativeRuntime =
  | {
      platform: "linux";
      arch: NodeJS.Architecture;
      /** `null` when the C library could not be identified, so no native binary is known to run. */
      libc: LinuxLibc | null;
    }
  | {
      platform: Exclude<NodeJS.Platform, "linux">;
      arch: NodeJS.Architecture;
    };

/** Major and minor numbers at the start of the version that glibc reports, such as `2.35`. */
const GLIBC_VERSION_PATTERN = /^(\d+)\.(\d+)/u;

/**
 * Identifies the C library from the `glibcVersionRuntime` of a Node diagnostic report, which only
 * a Node linked against glibc sets.
 */
export function linuxLibcFromReportedGlibcVersion(reported: string | undefined): LinuxLibc | null {
  if (reported === undefined) {
    return { family: "musl" };
  }
  const match = GLIBC_VERSION_PATTERN.exec(reported);
  if (match === null) {
    return null;
  }
  return { family: "gnu", version: { major: Number(match[1]), minor: Number(match[2]) } };
}

/** Detects the C library of the Linux system that Node is running on. */
function detectLinuxLibc(): LinuxLibc | null {
  // The report otherwise lists every network interface, which is slow to gather and never read.
  // `@types/node` does not declare `excludeNetwork`, which Node has had since version 22.0.
  const report = process.report as NodeJS.ProcessReport & { excludeNetwork: boolean };
  const excludedNetwork = report.excludeNetwork;
  report.excludeNetwork = true;
  try {
    const { header } = report.getReport() as { header?: { glibcVersionRuntime?: string } };
    return linuxLibcFromReportedGlibcVersion(header?.glibcVersionRuntime);
  } finally {
    report.excludeNetwork = excludedNetwork;
  }
}

/** Returns the current runtime dimensions used to select an optional native package. */
export function currentNativeRuntime(): NativeRuntime {
  const platform = process.platform;
  if (platform === "linux") {
    return {
      platform,
      arch: process.arch,
      libc: detectLinuxLibc(),
    };
  }

  return {
    platform,
    arch: process.arch,
  };
}

/** Returns whether a system with `libc` can load a Linux binary built for `family`. */
function canLoad(libc: LinuxLibc | null, family: LibcFamily): boolean {
  if (libc === null || libc.family !== family) {
    return false;
  }
  return (
    libc.family === "musl"
    || libc.version.major > GLIBC_FLOOR.major
    || (libc.version.major === GLIBC_FLOOR.major && libc.version.minor >= GLIBC_FLOOR.minor)
  );
}

/**
 * Resolves the native distribution package for the current Node platform and architecture, or
 * `null` when none of them runs there.
 */
export function resolveNativePackageName(
  runtime: NativeRuntime = currentNativeRuntime(),
): NativePackageName | null {
  for (const definition of NATIVE_RUNTIME_PACKAGE_DEFINITIONS) {
    if (definition.platform !== runtime.platform || definition.arch !== runtime.arch) {
      continue;
    }
    if (
      runtime.platform === "linux"
      && (!("libcFamily" in definition) || !canLoad(runtime.libc, definition.libcFamily))
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
