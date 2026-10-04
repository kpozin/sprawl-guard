import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  NATIVE_RUNTIME_PACKAGE_DEFINITIONS,
  RUNTIME_PACKAGE_DEFINITIONS,
} from "../sprawl-guard/src/lib/runtime-packages.ts";

interface CiRuntimeTarget {
  readonly runner: string;
  readonly rust_target: string;
  readonly built_binary_path: string;
}

interface ReleaseRuntimeTarget {
  readonly package_target: string;
  readonly runner: string;
  readonly rust_target: string;
  readonly built_artifact_path: string;
  readonly artifact_file_name: string;
}

/** Builds the CI native-runtime matrix from the authoritative runtime target manifest. */
export function ciRuntimeTargets(): readonly CiRuntimeTarget[] {
  return NATIVE_RUNTIME_PACKAGE_DEFINITIONS.map((definition) => ({
    runner: definition.runner,
    rust_target: definition.rustTarget,
    built_binary_path: `target/${definition.rustTarget}/debug/${definition.artifactRelativePath}`,
  }));
}

/** Builds the release artifact matrix from the authoritative runtime target manifest. */
export function releaseRuntimeTargets(): readonly ReleaseRuntimeTarget[] {
  return RUNTIME_PACKAGE_DEFINITIONS.map((definition) => ({
    package_target: definition.slug,
    runner: definition.runner,
    rust_target: definition.rustTarget,
    built_artifact_path: `target/${definition.rustTarget}/release/${definition.artifactRelativePath}`,
    artifact_file_name: definition.artifactRelativePath,
  }));
}

function writeGithubOutput(name: string, value: unknown): void {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (outputPath === undefined) {
    throw new Error("GITHUB_OUTPUT is required when emitting a workflow matrix");
  }
  appendFileSync(outputPath, `${name}=${JSON.stringify(value)}\n`);
}

function main(mode: string | undefined): void {
  switch (mode) {
    case "ci":
      writeGithubOutput("matrix", ciRuntimeTargets());
      break;
    case "release":
      writeGithubOutput("matrix", releaseRuntimeTargets());
      break;
    default:
      throw new Error(`expected matrix mode "ci" or "release", got ${String(mode)}`);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv[2]);
}
