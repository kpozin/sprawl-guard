import { spawnSync } from "node:child_process";
import { z } from "zod";
import { parseReleaseTag, type ReleaseTag } from "./release-policy.ts";

const GIT_COMMAND_MAXIMUM_OUTPUT_BYTES = 1_048_576;
const GIT_COMMAND_TIMEOUT_MILLISECONDS = 30_000;

/** Full commit recorded when a release candidate is frozen. */
export const RELEASE_COMMIT_SCHEMA = z
  .string()
  .regex(/^[0-9a-f]{40}$/u)
  .brand<"ReleaseCommit">();

/** Full commit recorded when a release candidate is frozen. */
export type ReleaseCommit = z.infer<typeof RELEASE_COMMIT_SCHEMA>;

/** Immutable source identity required by a local publication command. */
export interface ReleaseSourceExpectation {
  readonly commit: ReleaseCommit;
  readonly tag: ReleaseTag;
}

export type ParseReleaseSourceExpectationResult =
  | { readonly kind: "ok"; readonly expectation: ReleaseSourceExpectation }
  | { readonly kind: "invalid"; readonly args: readonly string[] };

export type ValidateReleaseSourceResult =
  | { readonly kind: "ok" }
  | {
      readonly kind: "head_mismatch";
      readonly actualCommit: string;
      readonly expectedCommit: ReleaseCommit;
    }
  | {
      readonly kind: "tag_mismatch";
      readonly actualCommit: string;
      readonly expectedCommit: ReleaseCommit;
      readonly tag: ReleaseTag;
    }
  | { readonly kind: "dirty_tree"; readonly status: string };

export type InspectReleaseSourceResult =
  | ValidateReleaseSourceResult
  | { readonly kind: "git_failed"; readonly detail: string };

/** Parses the required full commit and v-prefixed release tag arguments. */
export function parseReleaseSourceExpectation(
  args: readonly string[],
  expectedVersion: string,
): ParseReleaseSourceExpectationResult {
  if (args.length !== 2) {
    return { kind: "invalid", args };
  }
  const commit = RELEASE_COMMIT_SCHEMA.safeParse(args[0]);
  const tag = parseReleaseTag(args[1] ?? "");
  if (
    !commit.success ||
    tag.kind !== "ok" ||
    tag.releaseVersion !== expectedVersion
  ) {
    return { kind: "invalid", args };
  }
  return {
    kind: "ok",
    expectation: {
      commit: commit.data,
      tag: tag.releaseTag,
    },
  };
}

/** Validates a checkout snapshot against an immutable release expectation. */
export function validateReleaseSource(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly headCommit: string;
  readonly status: string;
  readonly tagCommit: string;
}): ValidateReleaseSourceResult {
  if (args.headCommit !== args.expectation.commit) {
    return {
      kind: "head_mismatch",
      actualCommit: args.headCommit,
      expectedCommit: args.expectation.commit,
    };
  }
  if (args.tagCommit !== args.expectation.commit) {
    return {
      kind: "tag_mismatch",
      actualCommit: args.tagCommit,
      expectedCommit: args.expectation.commit,
      tag: args.expectation.tag,
    };
  }
  if (args.status.length > 0) {
    return { kind: "dirty_tree", status: args.status };
  }
  return { kind: "ok" };
}

/** Reads and validates HEAD, the release tag, and tracked tree state. */
export function inspectReleaseSource(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly workspaceRoot: string;
}): InspectReleaseSourceResult {
  const headCommit = runGit(args.workspaceRoot, ["rev-parse", "HEAD"]);
  const tagCommit = runGit(args.workspaceRoot, [
    "rev-parse",
    `${args.expectation.tag}^{commit}`,
  ]);
  const status = runGit(args.workspaceRoot, [
    "status",
    "--porcelain",
    "--untracked-files=all",
  ]);
  const failed = [headCommit, tagCommit, status].find(
    (result) => result.kind === "failed",
  );
  if (failed?.kind === "failed") {
    return { kind: "git_failed", detail: failed.detail };
  }
  if (
    headCommit.kind !== "ok" ||
    tagCommit.kind !== "ok" ||
    status.kind !== "ok"
  ) {
    return { kind: "git_failed", detail: "git inspection failed" };
  }
  return validateReleaseSource({
    expectation: args.expectation,
    headCommit: headCommit.stdout,
    status: status.stdout,
    tagCommit: tagCommit.stdout,
  });
}

/** Result of one Git command; a failure keeps the exit status, which some commands use as an answer. */
export type GitResult =
  | { readonly kind: "ok"; readonly stdout: string }
  | {
      readonly kind: "failed";
      readonly detail: string;
      readonly exitCode: number | null;
    };

/** Runs one Git command in a repository with bounded time and output. */
export function runGit(workspaceRoot: string, args: readonly string[]): GitResult {
  const result = spawnSync("git", args, {
    cwd: workspaceRoot,
    encoding: "utf8",
    maxBuffer: GIT_COMMAND_MAXIMUM_OUTPUT_BYTES,
    stdio: ["ignore", "pipe", "pipe"],
    timeout: GIT_COMMAND_TIMEOUT_MILLISECONDS,
  });
  if (result.status === 0) {
    return { kind: "ok", stdout: result.stdout.trim() };
  }
  return {
    kind: "failed",
    exitCode: result.status,
    detail: [result.stdout, result.stderr, result.error?.message]
      .filter(
        (value): value is string => value !== undefined && value.length > 0,
      )
      .join("\n"),
  };
}
