import { fileURLToPath } from "node:url";
import {
  RUNTIME_PACKAGE_DEFINITIONS as SHARED_RUNTIME_PACKAGE_DEFINITIONS,
  type NativeRuntimePackageDefinition as SharedNativeRuntimePackageDefinition,
  type RuntimePackageDefinition as SharedRuntimePackageDefinition,
  type RuntimePackageName,
  type RuntimePackageSlug,
} from "../sprawl-guard/src/lib/runtime-packages.ts";
import { payloadFileRelativePath } from "./path-values.ts";
import {
  loadPublicPackageVersion,
  trustedPublicPackageVersion,
  type PublicPackageVersion,
} from "./package-version.ts";

type PublicScopedPackageName = `@kpozin/${string}`;
type RepositoryUrl = `https://github.com/${string}.git`;
type HomepageUrl = `https://github.com/${string}`;
type IssuesUrl = `https://github.com/${string}/issues`;
type LicenseExpression = "(MIT OR Apache-2.0)";
type LinuxLibc = "glibc" | "musl";
type NativePayloadPath = "bin/sprawl-guard" | "bin/sprawl-guard.exe";

type PackageManifestPath =
  | "sprawl-guard/package.json"
  | ".stage/package.json"
  | ".stage/sprawl-guard/package.json"
  | `.stage/packages/${RuntimePackageSlug}/package.json`;
type PackageReadmePath = `.stage/packages/${RuntimePackageSlug}/README.md`;
type PublishWorkspacePath = PackageManifestPath | ".stage/pnpm-workspace.yaml" | PackageReadmePath;

interface RepositoryMetadata {
  readonly type: "git";
  readonly url: RepositoryUrl;
}

interface PublicPublishConfig {
  readonly access: "public";
}

interface NativePublicPublishConfig extends PublicPublishConfig {
  readonly executableFiles: readonly [NativePayloadPath];
}

interface SharedPackageManifestFields<TName extends PublicScopedPackageName> {
  readonly name: TName;
  readonly version: PublicPackageVersion;
  readonly description: string;
  readonly license: LicenseExpression;
  readonly repository: RepositoryMetadata;
}

interface SharedWrapperPackageManifestFields
  extends SharedPackageManifestFields<typeof WRAPPER_PACKAGE_NAME> {
  readonly type: "module";
  readonly packageManager: "pnpm@11.7.0";
  readonly homepage: HomepageUrl;
  readonly bugs: {
    readonly url: IssuesUrl;
  };
  readonly bin: {
    readonly "sprawl-guard": "./dist/bin/sprawl-guard.js";
  };
  readonly engines: {
    readonly node: ">=22";
  };
  readonly dependencies: {
    readonly "better-result": "2.9.2";
    readonly zod: "4.4.3";
  };
}

interface SourceWrapperPackageManifest extends SharedWrapperPackageManifestFields {
  readonly private: true;
  readonly files: readonly ["README.md", "dist/", "!dist/lib/*.test.js"];
  readonly scripts: {
    readonly build: "pnpm run clean && pnpm --dir .. exec tsc -p sprawl-guard/tsconfig.json";
    readonly clean: "node -e \"require('node:fs').rmSync('dist', { recursive: true, force: true })\"";
    readonly prepack: "pnpm build";
    readonly test: "node --test dist/lib/*.test.js";
  };
}

interface PublishWrapperPackageManifest extends SharedWrapperPackageManifestFields {
  readonly publishConfig: PublicPublishConfig;
  readonly files: typeof PUBLISH_WRAPPER_FILES;
  readonly optionalDependencies: RuntimePackageDependencies;
}

interface NativeRuntimePackageManifest
  extends SharedPackageManifestFields<RuntimePackageName> {
  readonly publishConfig: NativePublicPublishConfig;
  readonly os: readonly [SharedNativeRuntimePackageDefinition["platform"]];
  readonly cpu: readonly [SharedNativeRuntimePackageDefinition["arch"]];
  readonly libc?: readonly [LinuxLibc];
  readonly files: readonly ["README.md", "LICENSE-MIT", "LICENSE-APACHE", "bin/"];
}

interface WasiRuntimePackageManifest
  extends SharedPackageManifestFields<RuntimePackageName> {
  readonly publishConfig: PublicPublishConfig;
  readonly files: readonly [
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "sprawl-guard.wasm",
  ];
}

interface PublishWorkspaceRootManifest {
  readonly name: "@kpozin/sprawl-guard-publish-workspace";
  readonly private: true;
  readonly packageManager: "pnpm@11.7.0";
  readonly engines: {
    readonly node: ">=22";
  };
}

export type RuntimePackageDefinition = SharedRuntimePackageDefinition;
type NativeRuntimePackageDefinition = SharedNativeRuntimePackageDefinition;

export type RuntimePackageManifest =
  | NativeRuntimePackageManifest
  | WasiRuntimePackageManifest;

type RuntimePackageDependencies = {
  readonly [TName in RuntimePackageName]: PublicPackageVersion;
};
type MutablePartialRuntimePackageDependencies = {
  -readonly [TName in keyof RuntimePackageDependencies]?: RuntimePackageDependencies[TName];
};

/** One generated text file in either the source workspace or rendered publish workspace. */
export interface PackageTextFile<TPath extends string = string> {
  readonly relativePath: TPath;
  readonly text: string;
}

/** Checked-in wrapper package name for both the source and publish manifests. */
export const WRAPPER_PACKAGE_NAME = "@kpozin/sprawl-guard" as const;

/** Root directory for the generated publish workspace. */
export const PUBLISH_WORKSPACE_DIRECTORY = ".stage" as const;

const SOURCE_WRAPPER_MANIFEST_PATH = "sprawl-guard/package.json" as const;
/** Version shared by the npm wrapper and every target-specific runtime package. */
export const PACKAGE_VERSION = loadPublicPackageVersion(
  fileURLToPath(new URL("../package-version.json", import.meta.url)),
);
const BETTER_RESULT_VERSION = trustedPublicPackageVersion("2.9.2");
const ZOD_VERSION = trustedPublicPackageVersion("4.4.3");
/** License files copied into each publishable npm package. */
export const PUBLISH_LICENSE_FILE_NAMES = ["LICENSE-MIT", "LICENSE-APACHE"] as const;
const LICENSE_EXPRESSION = "(MIT OR Apache-2.0)" as const satisfies LicenseExpression;
const REPOSITORY_URL =
  "https://github.com/kpozin/sprawl-guard.git" as const satisfies RepositoryUrl;
const HOMEPAGE_URL =
  "https://github.com/kpozin/sprawl-guard" as const satisfies HomepageUrl;
const ISSUES_URL =
  "https://github.com/kpozin/sprawl-guard/issues" as const satisfies IssuesUrl;

const REPOSITORY_METADATA: RepositoryMetadata = {
  type: "git",
  url: REPOSITORY_URL,
};

const PUBLIC_PUBLISH_CONFIG: PublicPublishConfig = {
  access: "public",
};

const SHARED_WRAPPER_PACKAGE_MANIFEST_FIELDS: SharedWrapperPackageManifestFields = {
  name: WRAPPER_PACKAGE_NAME,
  version: PACKAGE_VERSION,
  description: "Node.js wrapper for the sprawl-guard CLI.",
  license: LICENSE_EXPRESSION,
  type: "module",
  packageManager: "pnpm@11.7.0",
  repository: REPOSITORY_METADATA,
  homepage: HOMEPAGE_URL,
  bugs: {
    url: ISSUES_URL,
  },
  bin: {
    "sprawl-guard": "./dist/bin/sprawl-guard.js",
  },
  engines: {
    node: ">=22",
  },
  dependencies: {
    "better-result": BETTER_RESULT_VERSION,
    zod: ZOD_VERSION,
  },
};

const PUBLISH_WORKSPACE_ROOT_MANIFEST: PublishWorkspaceRootManifest = {
  name: "@kpozin/sprawl-guard-publish-workspace",
  private: true,
  packageManager: "pnpm@11.7.0",
  engines: {
    node: ">=22",
  },
};

/** Runtime package metadata shared by manifest rendering and payload staging. */
export const RUNTIME_PACKAGE_DEFINITIONS = SHARED_RUNTIME_PACKAGE_DEFINITIONS;

/** Schema documents distributed with the user-facing npm wrapper. */
export const PUBLISH_WRAPPER_SCHEMAS = [
  {
    fileName: "sprawl-guard.schema.json",
    packagePath: payloadFileRelativePath("schema/sprawl-guard.schema.json"),
  },
  {
    fileName: "sprawl-guard.ratchet.schema.json",
    packagePath: payloadFileRelativePath("schema/sprawl-guard.ratchet.schema.json"),
  },
] as const;
const PUBLISH_WRAPPER_FILES = [
  "README.md",
  ...PUBLISH_LICENSE_FILE_NAMES,
  "schema/",
  "dist/",
  "!dist/lib/*.test.js",
] as const;

/** Renders the checked-in source wrapper manifest. */
export function renderSourceWrapperPackageManifest(): SourceWrapperPackageManifest {
  return {
    ...SHARED_WRAPPER_PACKAGE_MANIFEST_FIELDS,
    private: true,
    files: ["README.md", "dist/", "!dist/lib/*.test.js"],
    scripts: {
      build: "pnpm run clean && pnpm --dir .. exec tsc -p sprawl-guard/tsconfig.json",
      clean: "node -e \"require('node:fs').rmSync('dist', { recursive: true, force: true })\"",
      prepack: "pnpm build",
      test: "node --test dist/lib/*.test.js",
    },
  };
}

/** Renders the publish-time wrapper manifest for the staged workspace. */
export function renderPublishWrapperPackageManifest(): PublishWrapperPackageManifest {
  return {
    ...SHARED_WRAPPER_PACKAGE_MANIFEST_FIELDS,
    publishConfig: PUBLIC_PUBLISH_CONFIG,
    files: PUBLISH_WRAPPER_FILES,
    optionalDependencies: renderRuntimePackageDependencies(),
  };
}

/** Renders one publish-time runtime package manifest from the target definition. */
export function renderRuntimePackageManifest(
  definition: RuntimePackageDefinition,
): RuntimePackageManifest {
  if (definition.kind === "wasi") {
    return {
      name: definition.packageName,
      version: PACKAGE_VERSION,
      description: "WASI fallback package for sprawl-guard.",
      license: LICENSE_EXPRESSION,
      repository: REPOSITORY_METADATA,
      publishConfig: PUBLIC_PUBLISH_CONFIG,
      files: ["README.md", ...PUBLISH_LICENSE_FILE_NAMES, "sprawl-guard.wasm"],
    };
  }

  const libc = runtimePackageLibc(definition);
  return {
    name: definition.packageName,
    version: PACKAGE_VERSION,
    description: renderNativeRuntimePackageDescription(definition),
    license: LICENSE_EXPRESSION,
    repository: REPOSITORY_METADATA,
    publishConfig: {
      ...PUBLIC_PUBLISH_CONFIG,
      executableFiles: [definition.payloadPath],
    },
    os: [definition.platform],
    cpu: [definition.arch],
    ...(libc === undefined ? {} : { libc: [libc] }),
    files: ["README.md", ...PUBLISH_LICENSE_FILE_NAMES, "bin/"],
  };
}

/** Renders the only checked-in generated manifest in the source workspace. */
export function sourceWrapperManifestFile(): PackageTextFile<typeof SOURCE_WRAPPER_MANIFEST_PATH> {
  return {
    relativePath: SOURCE_WRAPPER_MANIFEST_PATH,
    text: packageManifestJson(renderSourceWrapperPackageManifest()),
  };
}

/** Renders every generated text file needed to create the staged publish workspace. */
export function publishWorkspaceTextFiles(): readonly [
  PackageTextFile<".stage/package.json">,
  PackageTextFile<".stage/pnpm-workspace.yaml">,
  PackageTextFile<".stage/sprawl-guard/package.json">,
  ...PackageTextFile<PublishWorkspacePath>[],
] {
  return [
    {
      relativePath: `${PUBLISH_WORKSPACE_DIRECTORY}/package.json`,
      text: packageManifestJson(PUBLISH_WORKSPACE_ROOT_MANIFEST),
    },
    {
      relativePath: `${PUBLISH_WORKSPACE_DIRECTORY}/pnpm-workspace.yaml`,
      text: ['packages:', '  - "sprawl-guard"', '  - "packages/*"', ""].join("\n"),
    },
    {
      relativePath: `${PUBLISH_WORKSPACE_DIRECTORY}/sprawl-guard/package.json`,
      text: packageManifestJson(renderPublishWrapperPackageManifest()),
    },
    ...RUNTIME_PACKAGE_DEFINITIONS.map((definition) => ({
      relativePath:
        `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/package.json` as const,
      text: packageManifestJson(renderRuntimePackageManifest(definition)),
    })),
    ...RUNTIME_PACKAGE_DEFINITIONS.map((definition) => ({
      relativePath:
        `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/README.md` as const,
      text: renderRuntimePackageReadme(definition),
    })),
  ];
}

function renderRuntimePackageDependencies(): RuntimePackageDependencies {
  const dependencies: MutablePartialRuntimePackageDependencies = {};
  const packageNames = new Set<RuntimePackageName>();

  for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
    const packageName = definition.packageName;
    if (packageNames.has(packageName)) {
      throw new Error(`duplicate runtime package name: ${packageName}`);
    }
    packageNames.add(packageName);
    dependencies[packageName] = PACKAGE_VERSION;
  }

  if (packageNames.size !== RUNTIME_PACKAGE_DEFINITIONS.length) {
    throw new Error("runtime package dependency count did not match target count");
  }

  return dependencies as RuntimePackageDependencies;
}

function renderNativeRuntimePackageDescription(
  definition: NativeRuntimePackageDefinition,
): string {
  const libc = runtimePackageLibc(definition);
  const variantDescriptor =
    libc === undefined
      ? definition.slug === "win32-x64-msvc"
        ? " msvc"
        : ""
      : ` ${libc}`;
  return `Native ${definition.platform} ${definition.arch}${variantDescriptor} binary package for sprawl-guard.`;
}

function renderRuntimePackageReadme(
  definition: RuntimePackageDefinition,
): string {
  return [
    `# \`${definition.packageName}\``,
    "",
    renderRuntimePackageReadmeBody(definition),
    "Install `@kpozin/sprawl-guard` instead; it selects this package automatically when needed.",
    "",
    "See the [sprawl-guard project](https://github.com/kpozin/sprawl-guard#readme).",
    "",
  ].join("\n");
}

function renderRuntimePackageReadmeBody(
  definition: RuntimePackageDefinition,
): string {
  if (definition.kind === "wasi") {
    return "This package carries the WASI fallback module for `sprawl-guard`.";
  }

  return `This package carries the ${renderNativeRuntimePackageLabel(definition)} native \`sprawl-guard\` binary.`;
}

function renderNativeRuntimePackageLabel(
  definition: NativeRuntimePackageDefinition,
): string {
  const libc = runtimePackageLibc(definition);
  const libcLabel =
    libc === undefined
      ? definition.slug === "win32-x64-msvc"
        ? " msvc"
        : ""
      : ` ${libc}`;
  return `${definition.platform} ${definition.arch}${libcLabel}`;
}

function runtimePackageLibc(
  definition: NativeRuntimePackageDefinition,
): LinuxLibc | undefined {
  if (!("libcFamily" in definition)) {
    return undefined;
  }
  return definition.libcFamily === "gnu" ? "glibc" : definition.libcFamily;
}

function packageManifestJson(
  manifest:
    | PublishWorkspaceRootManifest
    | SourceWrapperPackageManifest
    | PublishWrapperPackageManifest
    | RuntimePackageManifest,
): string {
  return `${JSON.stringify(manifest, null, 2)}\n`;
}
