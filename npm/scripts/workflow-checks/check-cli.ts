#!/usr/bin/env node

/**
 * Checks that a Sprawl Guard command prints exactly the public package version, then runs its
 * help.
 */

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { PACKAGE_VERSION } from "../package-manifests.ts";
import type { PublicPackageVersion } from "../package-version.ts";

const USAGE = "usage: check-cli.ts -- COMMAND [ARGS...]";
/** The outcome of running one command. */
export type CommandOutput =
  | { readonly kind: "ok"; readonly stderr: string; readonly stdout: string }
  | { readonly kind: "failed"; readonly detail: string };

/** Runs a command and returns its output. */
export type RunCommand = (args: readonly string[]) => CommandOutput;

/** The result of checking a command. */
export type CheckCliResult =
  | { readonly kind: "ok" }
  | { readonly kind: "command_failed"; readonly args: readonly string[]; readonly detail: string }
  | { readonly kind: "unexpected_version_output"; readonly stderr: string; readonly stdout: string };

/** Runs `command --version`, requiring exact output, then `command --help`. */
export function checkCli(
  command: readonly string[],
  expectedVersion: PublicPackageVersion,
  run: RunCommand,
): CheckCliResult {
  const versionArgs = [...command, "--version"];
  const version = run(versionArgs);
  if (version.kind === "failed") {
    return { kind: "command_failed", args: versionArgs, detail: version.detail };
  }
  if (version.stdout !== `sprawl-guard ${expectedVersion}\n` || version.stderr !== "") {
    return { kind: "unexpected_version_output", stderr: version.stderr, stdout: version.stdout };
  }
  const helpArgs = [...command, "--help"];
  const help = run(helpArgs);
  return help.kind === "failed"
    ? { kind: "command_failed", args: helpArgs, detail: help.detail }
    : { kind: "ok" };
}

/** Runs a command without a shell and captures its output. */
function runCommand(args: readonly string[]): CommandOutput {
  const [command, ...rest] = args;
  if (command === undefined) {
    return { kind: "failed", detail: "empty command" };
  }
  const result = spawnSync(command, rest, { encoding: "utf8" });
  if (result.error !== undefined) {
    return { kind: "failed", detail: result.error.message };
  }
  return result.status === 0
    ? { kind: "ok", stderr: result.stderr, stdout: result.stdout }
    : { kind: "failed", detail: `${args.join(" ")} exited with ${result.status ?? result.signal}` };
}

/** Explains a failed check in one line. */
function describeFailure(failure: Exclude<CheckCliResult, { readonly kind: "ok" }>): string {
  switch (failure.kind) {
    case "command_failed":
      return `${failure.args.join(" ")} failed: ${failure.detail}`;
    case "unexpected_version_output":
      return `--version printed ${JSON.stringify(failure.stdout)} with stderr ${JSON.stringify(failure.stderr)}`;
  }
}

function main(): number {
  const separator = process.argv.indexOf("--", 2);
  const command = separator === -1 ? [] : process.argv.slice(separator + 1);
  if (separator !== 2 || command.length === 0) {
    console.error(USAGE);
    return 2;
  }
  const result = checkCli(command, PACKAGE_VERSION, runCommand);
  if (result.kind !== "ok") {
    console.error(describeFailure(result));
    return 1;
  }
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
