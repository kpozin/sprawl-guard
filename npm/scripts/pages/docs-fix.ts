#!/usr/bin/env node

/**
 * Publishes a documentation fix for the latest release without a new release.
 *
 * A fix is a commit on `main` whose changes to the documentation site go out before the next
 * release. Only those changes are applied, onto `docs/<release tag>`: a branch that starts at the
 * release tag and collects such fixes, so the deployed site keeps describing the released version
 * rather than unreleased work on `main`. The commit's other changes wait for the next release.
 *
 * The owner creates each release's branch. GitHub refuses to let the workflow's token create a
 * branch whose workflow files differ from the default branch's, as a release tag's usually do. The
 * token may still add commits that leave workflow files unchanged.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
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

/**
 * The documentation site's directory: its pages, images, static files, configuration and build
 * scripts. Only the deployed site is built from it, so a documentation fix may change anything here.
 */
export const DOCUMENTATION_DIRECTORY = "site/";

/** Git options that record applied commits as GitHub's Actions bot. */
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
  /**
   * A directory in a clean checkout whose `origin` remote is the public repository. Git runs from
   * the checkout's top level, because paths in this script are relative to it.
   */
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
  | { readonly kind: "docs_branch_missing"; readonly releaseTag: ReleaseTag }
  | {
      readonly kind: "docs_branch_not_from_release";
      readonly branchHead: ReleaseCommit;
      readonly releaseTag: ReleaseTag;
    }
  | { readonly kind: "no_documentation_changes"; readonly docsCommit: ReleaseCommit }
  | { readonly kind: "cherry_pick_failed"; readonly detail: string }
  | { readonly kind: "non_documentation_paths"; readonly paths: readonly string[] }
  | GitFailure;

/**
 * Applies a commit's documentation changes onto the release's existing documentation branch and
 * pushes it. The branch is pushed only if its whole difference from the release tag stays inside the
 * documentation directory, which also catches commits pushed to the branch by hand.
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
  const topLevel = runGit(args.repository, ["rev-parse", "--show-toplevel"]);
  if (topLevel.kind === "failed") {
    return gitFailure("rev-parse", topLevel.detail);
  }
  const git = (...gitArgs: readonly string[]) => runGit(topLevel.stdout, gitArgs);
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
  if (existingBranch.kind === "failed") {
    return { kind: "docs_branch_missing", releaseTag: release.releaseTag };
  }
  const branchHead = RELEASE_COMMIT_SCHEMA.safeParse(existingBranch.stdout);
  if (!branchHead.success) {
    return gitFailure("rev-parse", `not a full commit SHA: ${existingBranch.stdout}`);
  }
  const fromRelease = ancestry(git, release.releaseRef, branchHead.data);
  if (fromRelease.kind === "git_failed") {
    return fromRelease;
  }
  if (fromRelease.kind === "not_ancestor") {
    return { kind: "docs_branch_not_from_release", branchHead: branchHead.data, releaseTag: release.releaseTag };
  }
  const checkedOut = git("checkout", "--quiet", "--detach", branchHead.data);
  if (checkedOut.kind === "failed") {
    return gitFailure("checkout", checkedOut.detail);
  }

  const history = git("log", "--format=%B", `${release.releaseRef}..HEAD`);
  if (history.kind === "failed") {
    return gitFailure("log", history.detail);
  }
  const alreadyApplied = history.stdout.includes(`(cherry picked from commit ${docsCommit.data})`);
  if (!alreadyApplied) {
    const picked = pickDocumentation(git, docsCommit.data);
    if (picked.kind !== "ok") {
      git("reset", "--quiet", "--hard", "HEAD");
      return picked;
    }
  }

  // Without rename detection a move lists both its paths, and `-z` keeps names unquoted.
  const changed = git("diff", "--name-only", "--no-renames", "-z", release.releaseRef, "HEAD");
  if (changed.kind === "failed") {
    return gitFailure("diff", changed.detail);
  }
  const outside = changed.stdout
    .split("\0")
    .filter(
      (file) => file.length > 0 && !file.startsWith(DOCUMENTATION_DIRECTORY),
    );
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

type PickResult =
  | { readonly kind: "ok" }
  | { readonly kind: "no_documentation_changes"; readonly docsCommit: ReleaseCommit }
  | { readonly kind: "cherry_pick_failed"; readonly detail: string }
  | GitFailure;

/**
 * Commits the documentation part of `commit` on top of `HEAD`, with the commit's author and message
 * and the `(cherry picked from commit …)` line that `git cherry-pick -x` writes. The patch is limited
 * to the documentation directory, so the commit's other changes can't conflict.
 */
function pickDocumentation(git: (...gitArgs: readonly string[]) => GitResult, commit: ReleaseCommit): PickResult {
  const patch = git("diff", "--binary", "--no-renames", `${commit}^`, commit, "--", DOCUMENTATION_DIRECTORY);
  if (patch.kind === "failed") {
    return gitFailure("diff", patch.detail);
  }
  if (patch.stdout.length === 0) {
    return { kind: "no_documentation_changes", docsCommit: commit };
  }
  const patchDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "docs-fix-"));
  try {
    const patchFile = path.join(patchDirectory, "documentation.patch");
    // `runGit` trims output, and `git apply` needs the patch's final newline.
    fs.writeFileSync(patchFile, `${patch.stdout}\n`);
    const applied = git("apply", "--3way", "--index", patchFile);
    if (applied.kind === "failed") {
      return { kind: "cherry_pick_failed", detail: applied.detail };
    }
  } finally {
    fs.rmSync(patchDirectory, { force: true, recursive: true });
  }

  const message = git("log", "-1", "--format=%B", commit);
  if (message.kind === "failed") {
    return gitFailure("log", message.detail);
  }
  // `--reuse-message` keeps the original author and date; `--amend` then adds the provenance line.
  const committed = git(...COMMITTER_OPTIONS, "commit", "--quiet", "--reuse-message", commit);
  if (committed.kind === "failed") {
    return gitFailure("commit", committed.detail);
  }
  const marked = git(
    ...COMMITTER_OPTIONS,
    "commit",
    "--quiet",
    "--amend",
    "--message",
    `${message.stdout}\n\n(cherry picked from commit ${commit})`,
  );
  return marked.kind === "failed" ? gitFailure("commit", marked.detail) : { kind: "ok" };
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
    case "docs_branch_missing":
      console.error(
        [
          `docs/${result.releaseTag} does not exist. Create it from a clone of this repository with:`,
          `git push origin '${result.releaseTag}^{commit}:refs/heads/docs/${result.releaseTag}'`,
        ].join("\n"),
      );
      return 1;
    case "docs_branch_not_from_release":
      console.error(
        [
          `docs/${result.releaseTag} (at ${result.branchHead}) does not start from ${result.releaseTag}.`,
          "Check which of its commits to keep. Then replace it, unless it has moved since, with:",
          `git push --force-with-lease=refs/heads/docs/${result.releaseTag}:${result.branchHead} origin '${result.releaseTag}^{commit}:refs/heads/docs/${result.releaseTag}'`,
          "and run the workflow again for each fix to keep.",
        ].join("\n"),
      );
      return 1;
    case "no_documentation_changes":
      console.error(`documentation commit changes nothing in ${DOCUMENTATION_DIRECTORY}: ${result.docsCommit}`);
      return 1;
    case "cherry_pick_failed":
      console.error(`documentation commit does not apply to the release's documentation:\n${result.detail}`);
      return 1;
    case "non_documentation_paths":
      console.error(
        [
          `the documentation branch may differ from its release only in ${DOCUMENTATION_DIRECTORY}, not:`,
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
