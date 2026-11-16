#!/usr/bin/env node

/**
 * Checks that a Sprawl Guard command follows directory links and reports link loops when
 * `traversal.follow_symlinks = "directories"`, so native and WASI builds can be compared on every
 * runner.
 */

import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";

const USAGE = "usage: check-directory-links.ts -- COMMAND [ARGS...]";
/** The exit status that `sprawl-guard check` uses when it finds violations. */
const VIOLATIONS_EXIT_STATUS = 1;

const CONFIG = `schema_version = 2

[limits.max_lines_per_file]
production = 1

[languages.rust]
enabled = true

[traversal]
follow_symlinks = "directories"
`;
/** Two lines, so the file breaks the one-line limit wherever the scan reaches it. */
const SOURCE = "fn first() {}\nfn second() {}\n";

/** The files the check must report, once through the real directory and once through the link. */
export const EXPECTED_VIOLATION_PATHS: readonly string[] = ["linked/lib.rs", "real/lib.rs"];
/** The loop warnings the check must report, one for each way the scan reaches the cycle link. */
export const EXPECTED_LOOP_PATHS: readonly string[] = ["linked/cycle", "real/cycle"];

const REPORT_SCHEMA = z.object({
  violations: z.array(z.object({ path: z.string() })),
  coverage: z.object({
    unreadable_paths: z.array(z.object({ kind: z.string(), path: z.string() })),
  }),
});

/** The result of checking a `check --format json` report. */
export type CheckDirectoryLinksResult =
  | { readonly kind: "ok" }
  | { readonly kind: "invalid_report"; readonly detail: string }
  | {
      readonly kind: "unexpected_paths";
      readonly violations: readonly string[];
      readonly loops: readonly string[];
    };

/** Checks that a report names the linked and real files and both routes into the link loop. */
export function checkDirectoryLinksReport(reportJson: string): CheckDirectoryLinksResult {
  let json: unknown;
  try {
    json = JSON.parse(reportJson);
  } catch (error) {
    return { kind: "invalid_report", detail: String(error) };
  }
  const report = REPORT_SCHEMA.safeParse(json);
  if (!report.success) {
    return { kind: "invalid_report", detail: report.error.message };
  }
  const violations = report.data.violations.map((violation) => violation.path).sort();
  const loops = report.data.coverage.unreadable_paths
    .filter((entry) => entry.kind === "symlink_loop")
    .map((entry) => entry.path)
    .sort();
  return isDeepStrictEqual(violations, EXPECTED_VIOLATION_PATHS) &&
    isDeepStrictEqual(loops, EXPECTED_LOOP_PATHS)
    ? { kind: "ok" }
    : { kind: "unexpected_paths", violations, loops };
}

/**
 * Writes a repository whose `linked` directory links to `real`, which holds a link back to the
 * repository root. Junctions work on Windows runners without symlink privileges; elsewhere Node.js
 * creates ordinary directory symlinks.
 */
function writeLinkedRepository(repository: string): void {
  mkdirSync(path.join(repository, "real"), { recursive: true });
  writeFileSync(path.join(repository, "sprawl-guard.toml"), CONFIG);
  writeFileSync(path.join(repository, "real", "lib.rs"), SOURCE);
  symlinkSync(path.join(repository, "real"), path.join(repository, "linked"), "junction");
  symlinkSync(repository, path.join(repository, "real", "cycle"), "junction");
}

/** Runs the check in a fresh linked repository and returns a failure message, if any. */
function runCheck(command: readonly string[]): string | undefined {
  const [program, ...args] = command;
  if (program === undefined) {
    return USAGE;
  }
  const repository = mkdtempSync(path.join(tmpdir(), "check-directory-links-"));
  try {
    writeLinkedRepository(repository);
    const result = spawnSync(
      program,
      [...args, "check", "--format", "json", "--unreadable-paths", "warn"],
      { cwd: repository, encoding: "utf8" },
    );
    if (result.error !== undefined) {
      return result.error.message;
    }
    if (result.status !== VIOLATIONS_EXIT_STATUS) {
      return `check exited with ${result.status ?? result.signal}, expected ${VIOLATIONS_EXIT_STATUS}\n${result.stderr}`;
    }
    return describeFailure(checkDirectoryLinksReport(result.stdout));
  } finally {
    rmSync(repository, { force: true, recursive: true });
  }
}

/** Explains a failed report check in one line. */
function describeFailure(result: CheckDirectoryLinksResult): string | undefined {
  switch (result.kind) {
    case "ok":
      return undefined;
    case "invalid_report":
      return `invalid check report: ${result.detail}`;
    case "unexpected_paths":
      return `check reported violations ${JSON.stringify(result.violations)} and loops ${JSON.stringify(result.loops)}, expected ${JSON.stringify(EXPECTED_VIOLATION_PATHS)} and ${JSON.stringify(EXPECTED_LOOP_PATHS)}`;
  }
}

function main(): number {
  const separator = process.argv.indexOf("--", 2);
  if (separator !== 2 || process.argv.length === separator + 1) {
    console.error(USAGE);
    return 2;
  }
  const failure = runCheck(process.argv.slice(separator + 1));
  if (failure !== undefined) {
    console.error(failure);
    return 1;
  }
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
