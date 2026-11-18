/** Package name for the WASI fallback distribution. */
export const WASM_PACKAGE_NAME = "@kpozin/sprawl-guard-wasm32-wasip1" as const;

/**
 * Container image that builds and tests the Linux glibc binaries. A binary needs at least the glibc
 * version it was linked against, so this image's glibc 2.35 is the oldest that the binaries
 * support. The digest pins a multi-architecture image, so one reference serves both Arm and Intel
 * runners.
 */
export const GLIBC_FLOOR_IMAGE =
  "ubuntu:22.04@sha256:9abcaffc92a5279fff3a6a41635c0800ea33e528425fb389658806476e7defde" as const;

type RuntimeRunner = `${"macos" | "ubuntu" | "windows"}-${string}`;
type NativeRustTarget =
  | `${string}-apple-darwin`
  | `${string}-unknown-linux-${"gnu" | "musl"}`
  | `${string}-pc-windows-msvc`;

interface CommonNativeRuntimePackageDefinitionInput {
  readonly slug: string;
  readonly arch: NodeJS.Architecture;
  readonly rustTarget: NativeRustTarget;
  readonly runner: RuntimeRunner;
}

interface PosixNonLinuxRuntimePackageDefinitionInput
  extends CommonNativeRuntimePackageDefinitionInput {
  readonly platform: Exclude<NodeJS.Platform, "linux" | "win32">;
  readonly libcFamily?: never;
  readonly container?: never;
}

/** A Linux binary linked against glibc, built in the glibc floor image so older systems can run it. */
interface LinuxGnuRuntimePackageDefinitionInput
  extends CommonNativeRuntimePackageDefinitionInput {
  readonly platform: "linux";
  readonly libcFamily: "gnu";
  readonly container: typeof GLIBC_FLOOR_IMAGE;
}

/** A statically linked musl binary, which runs on any Linux system and so builds on the runner. */
interface LinuxMuslRuntimePackageDefinitionInput
  extends CommonNativeRuntimePackageDefinitionInput {
  readonly platform: "linux";
  readonly libcFamily: "musl";
  readonly container?: never;
}

interface WindowsRuntimePackageDefinitionInput
  extends CommonNativeRuntimePackageDefinitionInput {
  readonly platform: "win32";
  readonly libcFamily?: never;
  readonly container?: never;
}

type PosixNativeRuntimePackageDefinitionInput =
  | PosixNonLinuxRuntimePackageDefinitionInput
  | LinuxGnuRuntimePackageDefinitionInput
  | LinuxMuslRuntimePackageDefinitionInput;
type NativeRuntimePackageDefinitionInput =
  | PosixNativeRuntimePackageDefinitionInput
  | WindowsRuntimePackageDefinitionInput;

type NativeRuntimePackageIdentity<TDefinition extends NativeRuntimePackageDefinitionInput> = {
  readonly kind: "native";
  readonly packageName: `@kpozin/sprawl-guard-${TDefinition["slug"]}`;
};

type PosixNativePayload = {
  readonly artifactRelativePath: "sprawl-guard";
  readonly payloadPath: "bin/sprawl-guard";
  readonly payloadCheckKind: "posix-executable";
};

type WindowsNativePayload = {
  readonly artifactRelativePath: "sprawl-guard.exe";
  readonly payloadPath: "bin/sprawl-guard.exe";
  readonly payloadCheckKind: "file";
};

function nativeRuntimePackage<const TDefinition extends PosixNativeRuntimePackageDefinitionInput>(
  definition: TDefinition,
): TDefinition & NativeRuntimePackageIdentity<TDefinition> & PosixNativePayload;
function nativeRuntimePackage<const TDefinition extends WindowsRuntimePackageDefinitionInput>(
  definition: TDefinition,
): TDefinition & NativeRuntimePackageIdentity<TDefinition> & WindowsNativePayload;
function nativeRuntimePackage(
  definition: NativeRuntimePackageDefinitionInput,
): NativeRuntimePackageDefinitionInput &
  NativeRuntimePackageIdentity<NativeRuntimePackageDefinitionInput> &
  (PosixNativePayload | WindowsNativePayload) {
  const identity = {
    kind: "native" as const,
    packageName: `@kpozin/sprawl-guard-${definition.slug}` as const,
  };
  if (definition.platform === "win32") {
    return {
      ...definition,
      ...identity,
      artifactRelativePath: "sprawl-guard.exe",
      payloadPath: "bin/sprawl-guard.exe",
      payloadCheckKind: "file",
    };
  }
  return {
    ...definition,
    ...identity,
    artifactRelativePath: "sprawl-guard",
    payloadPath: "bin/sprawl-guard",
    payloadCheckKind: "posix-executable",
  };
}

function wasiRuntimePackage() {
  return {
    kind: "wasi",
    slug: "wasm32-wasip1",
    packageName: WASM_PACKAGE_NAME,
    rustTarget: "wasm32-wasip1",
    runner: "ubuntu-24.04",
    artifactRelativePath: "sprawl-guard.wasm",
    payloadPath: "sprawl-guard.wasm",
    payloadCheckKind: "file",
  } as const;
}

/** Runtime package targets shared by wrapper resolution, publishing, and workflows. */
export const RUNTIME_PACKAGE_DEFINITIONS = [
  nativeRuntimePackage({
    slug: "darwin-arm64",
    platform: "darwin",
    arch: "arm64",
    rustTarget: "aarch64-apple-darwin",
    runner: "macos-15",
  }),
  nativeRuntimePackage({
    slug: "darwin-x64",
    platform: "darwin",
    arch: "x64",
    rustTarget: "x86_64-apple-darwin",
    runner: "macos-15-intel",
  }),
  nativeRuntimePackage({
    slug: "linux-arm64-gnu",
    platform: "linux",
    arch: "arm64",
    libcFamily: "gnu",
    rustTarget: "aarch64-unknown-linux-gnu",
    runner: "ubuntu-24.04-arm",
    container: GLIBC_FLOOR_IMAGE,
  }),
  nativeRuntimePackage({
    slug: "linux-x64-gnu",
    platform: "linux",
    arch: "x64",
    libcFamily: "gnu",
    rustTarget: "x86_64-unknown-linux-gnu",
    runner: "ubuntu-24.04",
    container: GLIBC_FLOOR_IMAGE,
  }),
  nativeRuntimePackage({
    slug: "linux-x64-musl",
    platform: "linux",
    arch: "x64",
    libcFamily: "musl",
    rustTarget: "x86_64-unknown-linux-musl",
    runner: "ubuntu-24.04",
  }),
  wasiRuntimePackage(),
  nativeRuntimePackage({
    slug: "win32-x64-msvc",
    platform: "win32",
    arch: "x64",
    rustTarget: "x86_64-pc-windows-msvc",
    runner: "windows-2025",
  }),
] as const;

/** Runtime package metadata shared across resolution, build, packaging, and workflows. */
export type RuntimePackageDefinition = (typeof RUNTIME_PACKAGE_DEFINITIONS)[number];

/** Native runtime package metadata used for host package selection. */
export type NativeRuntimePackageDefinition = Extract<
  RuntimePackageDefinition,
  { readonly kind: "native" }
>;

/** Slug used in package names, staged package paths, and artifact directories. */
export type RuntimePackageSlug = RuntimePackageDefinition["slug"];

/** Package name for a runtime distribution package. */
export type RuntimePackageName = RuntimePackageDefinition["packageName"];

/** Package names for supported native wrapper distributions. */
export type NativePackageName = NativeRuntimePackageDefinition["packageName"];

type NativeRuntimePackageDefinitionWithLibc = Extract<
  NativeRuntimePackageDefinition,
  { readonly libcFamily: "gnu" | "musl" }
>;

/** C library family used by Linux native distributions. */
export type LibcFamily = NativeRuntimePackageDefinitionWithLibc["libcFamily"];

/** Native package targets, excluding the WASI fallback package. */
export const NATIVE_RUNTIME_PACKAGE_DEFINITIONS =
  RUNTIME_PACKAGE_DEFINITIONS.filter(isNativeRuntimePackageDefinition);

function isNativeRuntimePackageDefinition(
  definition: RuntimePackageDefinition,
): definition is NativeRuntimePackageDefinition {
  return definition.kind === "native";
}
