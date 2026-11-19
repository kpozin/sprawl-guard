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
import { isDeepStrictEqual, parseArgs } from "node:util";
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

/** The paths one scan must report. */
export type Expectation = {
  /** Files over the limit, sorted. */
  readonly violations: readonly string[];
  /** Links that lead back into the scan, sorted. */
  readonly loops: readonly string[];
};

/**
 * A scan of the whole repository reports the file once through the real directory and once through
 * the link, and reaches the cycle link both ways.
 */
export const REPOSITORY_SCAN: Expectation = {
  violations: ["linked/lib.rs", "real/lib.rs"],
  loops: ["linked/cycle", "real/cycle"],
};

/**
 * A file selected by an absolute path through the link is reported at its resolved path, beside the
 * cycle link in the same directory.
 */
export const LINKED_FILE_SCAN: Expectation = { violations: ["real/lib.rs"], loops: ["real/cycle"] };

/** The exit status that the check script uses for a malformed command line. */
const USAGE_EXIT_STATUS = 2;

const REPORT_SCHEMA = z.object({
  violations: z.array(z.object({ path: z.string() })),
  coverage: z.object({
    unreadable_paths: z.array(z.object({ kind: z.string(), path: z.string(), detail: z.string() })),
  }),
});

/** The result of checking a `check --format json` report. */
export type CheckDirectoryLinksResult =
  | { readonly kind: "ok" }
  | { readonly kind: "invalid_report"; readonly detail: string }
  | {
      readonly kind: "unexpected_paths";
      readonly found: Expectation;
      /** Every unreadable path the check reported, to explain why a link was not followed. */
      readonly unreadable: readonly UnreadablePath[];
    };

/** One entry of the report's `coverage.unreadable_paths`. */
export type UnreadablePath = z.infer<typeof REPORT_SCHEMA>["coverage"]["unreadable_paths"][number];

/** Checks that a `check --format json` report names exactly the expected paths. */
export function checkDirectoryLinksReport(reportJson: string, expected: Expectation): CheckDirectoryLinksResult {
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
  const found = { violations, loops };
  return isDeepStrictEqual(found, expected)
    ? { kind: "ok" }
    : { kind: "unexpected_paths", found, unreadable: report.data.coverage.unreadable_paths };
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

/** Runs each scan in a fresh linked repository and returns the first failure message, if any. */
function runChecks(command: readonly string[]): string | undefined {
  const [program, ...args] = command;
  if (program === undefined) {
    return USAGE;
  }
  const repository = mkdtempSync(path.join(tmpdir(), "check-directory-links-"));
  try {
    writeLinkedRepository(repository);
    const scans: readonly (readonly [readonly string[], Expectation])[] = [
      [[], REPOSITORY_SCAN],
      [[path.join(repository, "linked", "lib.rs")], LINKED_FILE_SCAN],
    ];
    for (const [selected, expected] of scans) {
      const failure = runCheck(program, [...args, "check", "--format", "json", "--unreadable-paths", "warn", ...selected], repository, expected);
      if (failure !== undefined) {
        return `${[program, ...args, "check", ...selected].join(" ")}: ${failure}`;
      }
    }
    return undefined;
  } finally {
    rmSync(repository, { force: true, recursive: true });
  }
}

/** Runs one check in the repository and returns a failure message, if any. */
function runCheck(program: string, args: readonly string[], repository: string, expected: Expectation): string | undefined {
  const result = spawnSync(program, args, { cwd: repository, encoding: "utf8" });
  if (result.error !== undefined) {
    return result.error.message;
  }
  if (result.status !== VIOLATIONS_EXIT_STATUS) {
    return `check exited with ${result.status ?? result.signal}, expected ${VIOLATIONS_EXIT_STATUS}\n${result.stderr}`;
  }
  const failure = describeFailure(checkDirectoryLinksReport(result.stdout, expected), expected);
  return failure === undefined ? undefined : `${failure}\n${result.stderr}`;
}

/** Explains a failed report check in one line. */
function describeFailure(result: CheckDirectoryLinksResult, expected: Expectation): string | undefined {
  switch (result.kind) {
    case "ok":
      return undefined;
    case "invalid_report":
      return `invalid check report: ${result.detail}`;
    case "unexpected_paths":
      return `check reported ${JSON.stringify(result.found)}, expected ${JSON.stringify(expected)}; unreadable paths: ${JSON.stringify(result.unreadable)}`;
  }
}

function main(): number {
  // The wrapped command's own options follow the terminator, so nothing before it is parsed strictly.
  const { positionals, tokens } = parseArgs({ allowPositionals: true, strict: false, tokens: true });
  if (tokens[0]?.kind !== "option-terminator" || positionals.length === 0) {
    console.error(USAGE);
    return USAGE_EXIT_STATUS;
  }
  const failure = runChecks(positionals);
  if (failure !== undefined) {
    console.error(failure);
    return 1;
  }
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
