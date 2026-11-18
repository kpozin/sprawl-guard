import fs from "node:fs";
import path from "node:path";
import type { z } from "zod";
import type { AbsoluteDirectoryPath } from "../path-values.ts";
import type { PythonPackageVersion } from "./python-version.ts";
import type { WheelPlatformTag } from "./wheel-platforms.ts";

/** Name of the distribution on PyPI. */
const DISTRIBUTION_NAME = "sprawl-guard";
/** The distribution name as it appears in wheel file and directory names (PEP 427). */
const WHEEL_DISTRIBUTION_NAME = "sprawl_guard";
/** The one-line summary that PyPI shows, matching the Cargo crate's description. */
const SUMMARY = "Repository sprawl checker CLI.";
/** License files that every wheel carries, from the root of the public repository. */
export const WHEEL_LICENSE_FILE_NAMES = ["LICENSE-APACHE", "LICENSE-MIT"] as const;
/** Links shown in the sidebar of the PyPI project page. */
const PROJECT_URLS = [
  ["Documentation", "https://kpozin.github.io/sprawl-guard/"],
  ["Source", "https://github.com/kpozin/sprawl-guard"],
  ["Issues", "https://github.com/kpozin/sprawl-guard/issues"],
  ["Changelog", "https://github.com/kpozin/sprawl-guard/blob/main/CHANGELOG.md"],
] as const;

/** File name of the executable inside a wheel, which installers copy into the environment. */
export type WheelBinaryFileName = "sprawl-guard" | "sprawl-guard.exe";

/** Everything that goes into one wheel. */
export interface WheelContents {
  readonly version: PythonPackageVersion;
  readonly platformTag: WheelPlatformTag;
  readonly binaryPath: string;
  readonly binaryFileName: WheelBinaryFileName;
  /** The long description on PyPI, in Markdown. */
  readonly readme: string;
  /** Directory that holds {@link WHEEL_LICENSE_FILE_NAMES}. */
  readonly licenseDirectory: AbsoluteDirectoryPath;
}

/** The file name of a Sprawl Guard wheel, as {@link wheelFileName} spells it. */
export type WheelFileName = string & z.$brand<"WheelFileName">;

/** Returns the file name that `wheel pack` gives a wheel. */
export function wheelFileName(version: PythonPackageVersion, platformTag: WheelPlatformTag): WheelFileName {
  return `${WHEEL_DISTRIBUTION_NAME}-${version}-py3-none-${platformTag}.whl` as WheelFileName;
}

/**
 * Renders the wheel's core metadata (version 2.4). The wheel holds no Python code, so it sets no
 * `Requires-Python`.
 */
export function renderMetadata(version: PythonPackageVersion, readme: string): string {
  return [
    "Metadata-Version: 2.4",
    `Name: ${DISTRIBUTION_NAME}`,
    `Version: ${version}`,
    `Summary: ${SUMMARY}`,
    "License-Expression: MIT OR Apache-2.0",
    ...WHEEL_LICENSE_FILE_NAMES.map((name) => `License-File: ${name}`),
    ...PROJECT_URLS.map(([label, url]) => `Project-URL: ${label}, ${url}`),
    "Description-Content-Type: text/markdown",
    "",
    readme,
  ].join("\n");
}

/**
 * Renders the `WHEEL` file. `Root-Is-Purelib: false` marks the wheel as platform-specific, since it
 * holds a native binary.
 */
export function renderWheelFile(platformTag: WheelPlatformTag): string {
  return [
    "Wheel-Version: 1.0",
    `Generator: ${DISTRIBUTION_NAME}-release`,
    "Root-Is-Purelib: false",
    `Tag: py3-none-${platformTag}`,
    "",
  ].join("\n");
}

/**
 * Writes an unpacked wheel under `parentDirectory` and returns its root, which `wheel pack` turns
 * into the wheel archive. The binary goes under `.data/scripts/`, which installers copy into the
 * environment's `bin` or `Scripts` directory.
 */
export function writeWheelTree(parentDirectory: string, contents: WheelContents): string {
  const stem = `${WHEEL_DISTRIBUTION_NAME}-${contents.version}`;
  const root = path.join(parentDirectory, stem);
  const scriptsDirectory = path.join(root, `${stem}.data`, "scripts");
  const distInfoDirectory = path.join(root, `${stem}.dist-info`);
  const licensesDirectory = path.join(distInfoDirectory, "licenses");
  fs.mkdirSync(scriptsDirectory, { recursive: true });
  fs.mkdirSync(licensesDirectory, { recursive: true });

  const binaryPath = path.join(scriptsDirectory, contents.binaryFileName);
  fs.copyFileSync(contents.binaryPath, binaryPath);
  fs.chmodSync(binaryPath, 0o755);
  for (const name of WHEEL_LICENSE_FILE_NAMES) {
    fs.copyFileSync(path.join(contents.licenseDirectory, name), path.join(licensesDirectory, name));
  }
  fs.writeFileSync(
    path.join(distInfoDirectory, "METADATA"),
    renderMetadata(contents.version, contents.readme),
  );
  fs.writeFileSync(path.join(distInfoDirectory, "WHEEL"), renderWheelFile(contents.platformTag));
  return root;
}
