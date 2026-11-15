#!/usr/bin/env node

/**
 * Publishes a documentation fix for the latest release without a new release.
 *
 * A fix is a commit on `main` that changes only documentation content. It is cherry-picked onto
 * `docs/<release tag>`, a branch that starts at the release tag and collects such fixes, so the
 * deployed site keeps describing the released version rather than unreleased work on `main`.
 */

import fs from "node:fs";
import { fileURLToPath } from "node:url";
import {
  parseReleaseTag,
  type ParseReleaseTagResult,
  type ReleaseTag,
  type ReleaseVersion,
} from "../release-policy.ts";
import {
  RELEASE_COMMIT_SCHEMA,
  runGit,
  type GitResult,
  type ReleaseCommit,
} from "../release-source.ts";

/** The repository directory whose files a documentation fix may change. */
export const DOCUMENTATION_CONTENT_DIRECTORY = "site/src/content/";

/** Git options that record cherry-picked commits as GitHub's Actions bot. */
const COMMITTER_OPTIONS = [
  "-c",
  "user.name=github-actions[bot]",
  "-c",
  "user.email=41898282+github-actions[bot]@users.noreply.github.com",
] as const;

/** Where the fetch records the public repository's `main`. */
const MAIN_TRACKING_REF = "refs/remotes/origin/main";

/** Exit status of `git merge-base --is-ancestor` and `git rev-parse --verify --quiet` for "no". */
const GIT_NO_EXIT_CODE = 1;

/** What `applyDocsFix` publishes, and where. */
export interface ApplyDocsFixArgs {
  /** The full SHA of the commit on `main` to publish, not yet validated. */
  readonly docsCommit: string;
  /** The latest stable release's tag, not yet validated. */
  readonly releaseTag: string;
  /** A clean checkout whose `origin` remote is the public repository. */
  readonly repository: string;
}

/** The documentation branch's head after a fix, and the release it documents. */
interface DocsBranchHead {
  readonly commit: ReleaseCommit;
  readonly releaseTag: ReleaseTag;
  readonly releaseVersion: ReleaseVersion;
}

type GitFailure = {
  readonly kind: "git_failed";
  readonly command: string;
  readonly detail: string;
};

/** The documentation branch's new state, or why the fix was not published. */
export type ApplyDocsFixResult =
  | ({ readonly kind: "applied" } & DocsBranchHead)
  | ({ readonly kind: "already_applied" } & DocsBranchHead)
  | { readonly kind: "invalid_docs_commit"; readonly docsCommit: string }
  | Extract<ParseReleaseTagResult, { readonly kind: "invalid_release_tag" }>
  | { readonly kind: "docs_commit_not_on_main"; readonly docsCommit: ReleaseCommit }
  | { readonly kind: "docs_commit_released"; readonly docsCommit: ReleaseCommit }
  | { readonly kind: "cherry_pick_failed"; readonly detail: string }
  | { readonly kind: "non_documentation_paths"; readonly paths: readonly string[] }
  | GitFailure;

/**
 * Cherry-picks a documentation commit onto the release's documentation branch, creating the
 * branch from the release tag on first use, and pushes it. The branch is pushed only if its whole
 * difference from the release tag stays inside the documentation content directory.
 */
export function applyDocsFix(args: ApplyDocsFixArgs): ApplyDocsFixResult {
  const docsCommit = RELEASE_COMMIT_SCHEMA.safeParse(args.docsCommit);
  if (!docsCommit.success) {
    return { kind: "invalid_docs_commit", docsCommit: args.docsCommit };
  }
  const release = parseReleaseTag(args.releaseTag);
  if (release.kind !== "ok") {
    return release;
  }
  const git = (...gitArgs: readonly string[]) => runGit(args.repository, gitArgs);
  const branch = `docs/${release.releaseTag}`;
  const branchTrackingRef = `refs/remotes/origin/${branch}`;

  const fetched = git(
    "fetch",
    "--quiet",
    "--prune",
    "origin",
    `+refs/heads/main:${MAIN_TRACKING_REF}`,
    "+refs/heads/docs/*:refs/remotes/origin/docs/*",
    `+${release.releaseRef}:${release.releaseRef}`,
  );
  if (fetched.kind === "failed") {
    return gitFailure("fetch", fetched.detail);
  }

  const onMain = ancestry(git, docsCommit.data, MAIN_TRACKING_REF);
  if (onMain.kind === "git_failed") {
    return onMain;
  }
  if (onMain.kind === "not_ancestor") {
    return { kind: "docs_commit_not_on_main", docsCommit: docsCommit.data };
  }
  const released = ancestry(git, docsCommit.data, release.releaseRef);
  if (released.kind === "git_failed") {
    return released;
  }
  if (released.kind === "ancestor") {
    return { kind: "docs_commit_released", docsCommit: docsCommit.data };
  }

  const existingBranch = git("rev-parse", "--verify", "--quiet", `${branchTrackingRef}^{commit}`);
  if (existingBranch.kind === "failed" && existingBranch.exitCode !== GIT_NO_EXIT_CODE) {
    return gitFailure("rev-parse", existingBranch.detail);
  }
  const base = existingBranch.kind === "ok" ? branchTrackingRef : release.releaseRef;
  const checkedOut = git("checkout", "--quiet", "--detach", base);
  if (checkedOut.kind === "failed") {
    return gitFailure("checkout", checkedOut.detail);
  }

  const history = git("log", "--format=%B", `${release.releaseRef}..HEAD`);
  if (history.kind === "failed") {
    return gitFailure("log", history.detail);
  }
  const alreadyApplied = history.stdout.includes(`(cherry picked from commit ${docsCommit.data})`);
  if (!alreadyApplied) {
    const picked = git(...COMMITTER_OPTIONS, "cherry-pick", "-x", docsCommit.data);
    if (picked.kind === "failed") {
      git("cherry-pick", "--abort");
      return { kind: "cherry_pick_failed", detail: picked.detail };
    }
  }

  // Without rename detection a move lists both its paths, and `-z` keeps names unquoted.
  const changed = git("diff", "--name-only", "--no-renames", "-z", release.releaseRef, "HEAD");
  if (changed.kind === "failed") {
    return gitFailure("diff", changed.detail);
  }
  const outside = changed.stdout
    .split("\0")
    .filter((file) => file.length > 0 && !file.startsWith(DOCUMENTATION_CONTENT_DIRECTORY));
  if (outside.length > 0) {
    return { kind: "non_documentation_paths", paths: outside };
  }

  if (!alreadyApplied) {
    const pushed = git("push", "--quiet", "origin", `HEAD:refs/heads/${branch}`);
    if (pushed.kind === "failed") {
      return gitFailure("push", pushed.detail);
    }
  }
  const head = git("rev-parse", "HEAD");
  if (head.kind === "failed") {
    return gitFailure("rev-parse", head.detail);
  }
  const commit = RELEASE_COMMIT_SCHEMA.safeParse(head.stdout);
  if (!commit.success) {
    return gitFailure("rev-parse", `not a full commit SHA: ${head.stdout}`);
  }
  return {
    kind: alreadyApplied ? "already_applied" : "applied",
    commit: commit.data,
    releaseTag: release.releaseTag,
    releaseVersion: release.releaseVersion,
  };
}

/** Whether one commit is an ancestor of, or the same as, another. */
type Ancestry = { readonly kind: "ancestor" } | { readonly kind: "not_ancestor" } | GitFailure;

function ancestry(
  git: (...gitArgs: readonly string[]) => GitResult,
  ancestor: string,
  descendant: string,
): Ancestry {
  const result = git("merge-base", "--is-ancestor", ancestor, descendant);
  if (result.kind === "ok") {
    return { kind: "ancestor" };
  }
  return result.exitCode === GIT_NO_EXIT_CODE
    ? { kind: "not_ancestor" }
    : gitFailure("merge-base", result.detail);
}

function gitFailure(command: string, detail: string): GitFailure {
  return { kind: "git_failed", command: `git ${command}`, detail };
}

function main(): number {
  const result = applyDocsFix({
    docsCommit: process.env.DOCS_COMMIT ?? "",
    releaseTag: process.env.RELEASE_TAG ?? "",
    repository: process.cwd(),
  });
  switch (result.kind) {
    case "applied":
    case "already_applied":
      break;
    case "invalid_docs_commit":
      console.error(`documentation commit must be a full lowercase Git SHA: ${result.docsCommit}`);
      return 1;
    case "invalid_release_tag":
      console.error(`latest release tag is not a release version: ${result.releaseTag}`);
      return 1;
    case "docs_commit_not_on_main":
      console.error(`documentation commit is not on main: ${result.docsCommit}`);
      return 1;
    case "docs_commit_released":
      console.error(`documentation commit is already part of the latest release: ${result.docsCommit}`);
      return 1;
    case "cherry_pick_failed":
      console.error(`documentation commit does not apply to the release's documentation:\n${result.detail}`);
      return 1;
    case "non_documentation_paths":
      console.error(
        [
          `documentation fixes may change only ${DOCUMENTATION_CONTENT_DIRECTORY}, not:`,
          ...result.paths.map((file) => `- ${file}`),
        ].join("\n"),
      );
      return 1;
    case "git_failed":
      console.error(`${result.command} failed:\n${result.detail}`);
      return 1;
  }

  const githubOutputPath = process.env.GITHUB_OUTPUT;
  if (githubOutputPath === undefined || githubOutputPath.length === 0) {
    console.error("GITHUB_OUTPUT must name the GitHub Actions output file");
    return 1;
  }
  console.log(
    result.kind === "applied"
      ? `published ${result.commit} to docs/${result.releaseTag}`
      : `docs/${result.releaseTag} already contains the commit; deploying ${result.commit}`,
  );
  fs.appendFileSync(
    githubOutputPath,
    [`commit=${result.commit}`, `release_version=${result.releaseVersion}`, ""].join("\n"),
  );
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
