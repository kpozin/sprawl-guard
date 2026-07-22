/** Package name for the WASI fallback distribution. */
export const WASM_PACKAGE_NAME = "@kpozin/sprawl-guard-wasm32-wasip1" as const;

interface NonLinuxNativeRuntimePackageDefinitionInput {
  readonly kind: "native";
  readonly slug: string;
  readonly packageName: `@kpozin/sprawl-guard-${string}`;
  readonly platform: Exclude<NodeJS.Platform, "linux">;
  readonly arch: NodeJS.Architecture;
  readonly libcFamily?: never;
}

interface LinuxNativeRuntimePackageDefinitionInput {
  readonly kind: "native";
  readonly slug: string;
  readonly packageName: `@kpozin/sprawl-guard-${string}`;
  readonly platform: "linux";
  readonly arch: NodeJS.Architecture;
  readonly libcFamily: "gnu" | "musl";
}

interface WasiRuntimePackageDefinitionInput {
  readonly kind: "wasi";
  readonly slug: "wasm32-wasip1";
  readonly packageName: typeof WASM_PACKAGE_NAME;
}

type RuntimePackageDefinitionInput =
  | NonLinuxNativeRuntimePackageDefinitionInput
  | LinuxNativeRuntimePackageDefinitionInput
  | WasiRuntimePackageDefinitionInput;

/** Runtime package targets shared by the wrapper resolver and publish tooling. */
export const RUNTIME_PACKAGE_DEFINITIONS = [
  {
    kind: "native",
    slug: "darwin-arm64",
    packageName: "@kpozin/sprawl-guard-darwin-arm64",
    platform: "darwin",
    arch: "arm64",
  },
  {
    kind: "native",
    slug: "darwin-x64",
    packageName: "@kpozin/sprawl-guard-darwin-x64",
    platform: "darwin",
    arch: "x64",
  },
  {
    kind: "native",
    slug: "linux-arm64-gnu",
    packageName: "@kpozin/sprawl-guard-linux-arm64-gnu",
    platform: "linux",
    arch: "arm64",
    libcFamily: "gnu",
  },
  {
    kind: "native",
    slug: "linux-x64-gnu",
    packageName: "@kpozin/sprawl-guard-linux-x64-gnu",
    platform: "linux",
    arch: "x64",
    libcFamily: "gnu",
  },
  {
    kind: "native",
    slug: "linux-x64-musl",
    packageName: "@kpozin/sprawl-guard-linux-x64-musl",
    platform: "linux",
    arch: "x64",
    libcFamily: "musl",
  },
  {
    kind: "wasi",
    slug: "wasm32-wasip1",
    packageName: WASM_PACKAGE_NAME,
  },
  {
    kind: "native",
    slug: "win32-x64-msvc",
    packageName: "@kpozin/sprawl-guard-win32-x64-msvc",
    platform: "win32",
    arch: "x64",
  },
] as const satisfies readonly RuntimePackageDefinitionInput[];

/** Shared metadata for one runtime distribution package. */
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
