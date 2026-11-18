#!/usr/bin/env node

/**
 * Builds the PyPI wheels from the release binaries and checks each Linux wheel's platform tag
 * against the binary inside it.
 *
 * Needs `python` with the packages pinned in `python/release-tools.txt` and `auditwheel` on `PATH`.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { z } from "zod";
import { RUNTIME_PACKAGE_DEFINITIONS } from "../package-manifests.ts";
import { PUBLIC_PACKAGE_VERSION_SCHEMA } from "../package-version.ts";
import {
  absoluteDirectoryPath,
  artifactRootDirectory,
  type AbsoluteDirectoryPath,
  type ArtifactRootDirectory,
} from "../path-values.ts";
import { checkLinuxTag, describeLinuxTagGateFailure, type LinuxTagGateResult } from "./linux-tag-gate.ts";
import { pythonPackageVersion, type PythonPackageVersion } from "./python-version.ts";
import {
  linuxPolicyTag,
  WHEEL_PLATFORMS,
  wheelPlatformTag,
  type LinuxWheelPolicy,
  type NativeRuntimePackageSlug,
  type WheelPlatformTag,
} from "./wheel-platforms.ts";
import {
  renderMetadata,
  wheelFileName,
  writeWheelTree,
  type WheelBinaryFileName,
  type WheelFileName,
} from "./wheel-tree.ts";

const USAGE = "usage: build-wheels.ts ARTIFACT_DIRECTORY OUTPUT_DIRECTORY RELEASE_VERSION";
/** The positional arguments that {@link USAGE} names, in order. */
const ARGUMENTS_SCHEMA = z.tuple([
  z.string().min(1).transform(artifactRootDirectory),
  z.string().min(1).transform(absoluteDirectoryPath),
  PUBLIC_PACKAGE_VERSION_SCHEMA,
]);
/** Root of the public repository, which holds the license files and the PyPI README. */
const PUBLIC_ROOT = fileURLToPath(new URL("../../..", import.meta.url));
/** The PyPI project description, generated from the shared package README template. */
const README_PATH = path.join(PUBLIC_ROOT, "python", "README.md");

/**
 * Runs `wheel pack` and `auditwheel` against a binary-only wheel tagged older than any real
 * release binary supports, so the release fails if the gate stops rejecting such tags.
 */
const GATE_SELF_TEST = {
  slug: "linux-x64-gnu",
  policy: { family: "manylinux", major: 2, minor: 17, arch: "x86_64" },
} as const satisfies { slug: NativeRuntimePackageSlug; policy: LinuxWheelPolicy };

/** Validates core metadata read from standard input with the pinned `packaging` library. */
const VALIDATE_METADATA_SCRIPT = [
  "import sys",
  "from packaging.metadata import Metadata",
  "Metadata.from_email(sys.stdin.buffer.read(), validate=True)",
].join("\n");

/** The outcome of running one Python tool. */
export type ToolOutput =
  | { readonly kind: "ok"; readonly stdout: string }
  | { readonly kind: "failed"; readonly detail: string };

/** Runs a tool without a shell, optionally feeding it standard input. */
export type RunTool = (command: string, args: readonly string[], input?: string) => ToolOutput;

/** Inputs to {@link buildWheels}. */
export interface BuildWheelsArgs {
  /** Holds one `<slug>/<binary>` directory per runtime target, as `package-assembly` restores them. */
  readonly artifactDirectory: ArtifactRootDirectory;
  readonly outputDirectory: AbsoluteDirectoryPath;
  /** Scratch space for unpacked wheels and the gate's self-test wheel. */
  readonly workDirectory: AbsoluteDirectoryPath;
  readonly version: PythonPackageVersion;
  readonly readme: string;
  readonly licenseDirectory: AbsoluteDirectoryPath;
  readonly runTool: RunTool;
}

/** Why the wheels could not be built. */
export type BuildWheelsFailure =
  | { readonly kind: "invalid_metadata"; readonly detail: string }
  | { readonly kind: "pack_failed"; readonly fileName: WheelFileName; readonly detail: string }
  | { readonly kind: "missing_wheel"; readonly fileName: WheelFileName }
  | { readonly kind: "auditwheel_failed"; readonly fileName: WheelFileName; readonly detail: string }
  | {
      readonly kind: "tag_rejected";
      readonly fileName: WheelFileName;
      readonly result: Exclude<LinuxTagGateResult, { readonly kind: "ok" }>;
    }
  | {
      /** The gate did anything but reject the self-test wheel as declaring too old a policy. */
      readonly kind: "gate_self_test_failed";
      readonly result: Exclude<LinuxTagGateResult, { readonly kind: "declared_too_old" }>;
    };

/** The result of {@link buildWheels}: the built wheels' file names, or the first failure. */
export type BuildWheelsResult =
  | { readonly kind: "ok"; readonly fileNames: readonly WheelFileName[] }
  | BuildWheelsFailure;

/** One wheel to pack, possibly with a deliberately wrong Linux policy for the gate's self-test. */
interface WheelPlan {
  readonly slug: NativeRuntimePackageSlug;
  readonly platformTag: WheelPlatformTag;
  readonly destination: string;
}

type PackResult =
  | { readonly kind: "ok"; readonly fileName: WheelFileName; readonly wheelPath: string }
  | BuildWheelsFailure;

/**
 * Builds one wheel per native runtime target into the output directory. Each Linux wheel must pass
 * the tag gate, and the gate must reject a wheel whose tag is too old.
 */
export function buildWheels(args: BuildWheelsArgs): BuildWheelsResult {
  const metadataCheck = args.runTool(
    "python",
    ["-I", "-c", VALIDATE_METADATA_SCRIPT],
    renderMetadata(args.version, args.readme),
  );
  if (metadataCheck.kind === "failed") {
    return { kind: "invalid_metadata", detail: metadataCheck.detail };
  }

  const fileNames: WheelFileName[] = [];
  for (const [slug, platform] of Object.entries(WHEEL_PLATFORMS) as [
    NativeRuntimePackageSlug,
    (typeof WHEEL_PLATFORMS)[NativeRuntimePackageSlug],
  ][]) {
    const plan = { slug, platformTag: wheelPlatformTag(platform), destination: args.outputDirectory };
    const packed = packWheel(args, plan);
    if (packed.kind !== "ok") {
      return packed;
    }
    if (platform.kind === "linux") {
      const gate = runGate(args, packed.wheelPath, platform.policy);
      if (gate.kind === "auditwheel_failed") {
        return { ...gate, fileName: packed.fileName };
      }
      if (gate.kind !== "ok") {
        return { kind: "tag_rejected", fileName: packed.fileName, result: gate };
      }
    }
    fileNames.push(packed.fileName);
  }

  const probe = packWheel(args, {
    slug: GATE_SELF_TEST.slug,
    platformTag: linuxPolicyTag(GATE_SELF_TEST.policy),
    destination: path.join(args.workDirectory, "gate-self-test"),
  });
  if (probe.kind !== "ok") {
    return probe;
  }
  const probeResult = runGate(args, probe.wheelPath, GATE_SELF_TEST.policy);
  if (probeResult.kind === "auditwheel_failed") {
    return { ...probeResult, fileName: probe.fileName };
  }
  if (probeResult.kind !== "declared_too_old") {
    return { kind: "gate_self_test_failed", result: probeResult };
  }
  return { kind: "ok", fileNames };
}

function packWheel(args: BuildWheelsArgs, plan: WheelPlan): PackResult {
  const definition = RUNTIME_PACKAGE_DEFINITIONS.find(({ slug }) => slug === plan.slug);
  if (definition?.kind !== "native") {
    throw new Error(`no native runtime package for ${plan.slug}`);
  }
  const fileName = wheelFileName(args.version, plan.platformTag);
  const tree = writeWheelTree(path.join(args.workDirectory, "trees", plan.platformTag), {
    version: args.version,
    platformTag: plan.platformTag,
    binaryPath: path.join(args.artifactDirectory, plan.slug, definition.artifactRelativePath),
    binaryFileName: definition.artifactRelativePath satisfies WheelBinaryFileName,
    readme: args.readme,
    licenseDirectory: args.licenseDirectory,
  });
  fs.mkdirSync(plan.destination, { recursive: true });
  const packed = args.runTool("python", ["-I", "-m", "wheel", "pack", tree, "--dest-dir", plan.destination]);
  if (packed.kind === "failed") {
    return { kind: "pack_failed", fileName, detail: packed.detail };
  }
  const wheelPath = path.join(plan.destination, fileName);
  return fs.existsSync(wheelPath) ? { kind: "ok", fileName, wheelPath } : { kind: "missing_wheel", fileName };
}

function runGate(
  args: BuildWheelsArgs,
  wheelPath: string,
  declared: LinuxWheelPolicy,
): LinuxTagGateResult | { readonly kind: "auditwheel_failed"; readonly detail: string } {
  const report = args.runTool("auditwheel", ["show", "--json", wheelPath]);
  return report.kind === "failed"
    ? { kind: "auditwheel_failed", detail: report.detail }
    : checkLinuxTag(declared, report.stdout);
}

/** Explains a failed build in one line. */
export function describeBuildWheelsFailure(failure: BuildWheelsFailure): string {
  switch (failure.kind) {
    case "invalid_metadata":
      return `the wheel metadata is invalid: ${failure.detail}`;
    case "pack_failed":
      return `wheel pack failed for ${failure.fileName}: ${failure.detail}`;
    case "missing_wheel":
      return `wheel pack did not write ${failure.fileName}`;
    case "auditwheel_failed":
      return `auditwheel show failed for ${failure.fileName}: ${failure.detail}`;
    case "tag_rejected":
      return `${failure.fileName}: ${describeLinuxTagGateFailure(failure.result)}`;
    case "gate_self_test_failed":
      return `the tag gate accepted a wheel tagged ${linuxPolicyTag(GATE_SELF_TEST.policy)}, which is older than its binary supports (${failure.result.kind})`;
  }
}

/** Runs a tool without a shell and captures its standard output. */
function runTool(command: string, args: readonly string[], input?: string): ToolOutput {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    input,
    stdio: ["pipe", "pipe", "inherit"],
  });
  if (result.error !== undefined) {
    return { kind: "failed", detail: result.error.message };
  }
  return result.status === 0
    ? { kind: "ok", stdout: result.stdout }
    : { kind: "failed", detail: `${command} exited with ${result.status ?? result.signal}` };
}

function main(): number {
  let positionals: string[];
  try {
    ({ positionals } = parseArgs({ args: process.argv.slice(2), options: {}, allowPositionals: true, strict: true }));
  } catch (error) {
    console.error(`${String(error)}\n${USAGE}`);
    return 2;
  }
  const parsedArguments = ARGUMENTS_SCHEMA.safeParse(positionals);
  if (!parsedArguments.success) {
    console.error(`${z.prettifyError(parsedArguments.error)}\n${USAGE}`);
    return 2;
  }
  const [artifactDirectory, outputDirectory, releaseVersion] = parsedArguments.data;
  const version = pythonPackageVersion(releaseVersion);
  if (version.kind !== "ok") {
    console.error(`PyPI has no spelling for the prerelease in ${version.releaseVersion}`);
    return 1;
  }
  if (fs.existsSync(outputDirectory)) {
    console.error(`output directory already exists: ${outputDirectory}`);
    return 1;
  }

  const workDirectory = absoluteDirectoryPath(fs.mkdtempSync(path.join(os.tmpdir(), "sprawl-guard-wheels-")));
  try {
    const result = buildWheels({
      artifactDirectory,
      outputDirectory,
      workDirectory,
      version: version.version,
      readme: fs.readFileSync(README_PATH, "utf8"),
      licenseDirectory: absoluteDirectoryPath(PUBLIC_ROOT),
      runTool,
    });
    if (result.kind !== "ok") {
      console.error(describeBuildWheelsFailure(result));
      return 1;
    }
    for (const fileName of result.fileNames) {
      console.log(fileName);
    }
    return 0;
  } finally {
    fs.rmSync(workDirectory, { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
