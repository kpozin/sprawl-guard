import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, describe, it } from "node:test";
import { applyDocsFix } from "./docs-fix.ts";

const TEMPORARY_ROOT = mkdtempSync(path.join(tmpdir(), "docs-fix-"));
const RELEASE_TAG = "v1.2.3";
const DOCS_BRANCH = `refs/heads/docs/${RELEASE_TAG}`;
const GUIDE_PATH = "site/src/content/docs/guide.md";
/** A stylesheet whose last line is blank. */
const SPACED_PATH = "site/src/styles/spaced.css";

after(() => {
  rmSync(TEMPORARY_ROOT, { force: true, recursive: true });
});

function git(directory: string, ...args: readonly string[]): string {
  return execFileSync(
    "git",
    ["-c", "user.name=Test", "-c", "user.email=test@example.com", ...args],
    { cwd: directory, encoding: "utf8" },
  ).trim();
}

function commitFiles(directory: string, message: string, files: Readonly<Record<string, string>>): string {
  for (const [file, contents] of Object.entries(files)) {
    mkdirSync(path.dirname(path.join(directory, file)), { recursive: true });
    writeFileSync(path.join(directory, file), contents);
  }
  git(directory, "add", "--all");
  git(directory, "commit", "--quiet", "--message", message);
  return git(directory, "rev-parse", "HEAD");
}

/** Commits on the fixture's `main` after the release, and one on a feature branch. */
interface FixtureCommits {
  readonly asset: string;
  readonly code: string;
  readonly conflicting: string;
  readonly docs: string;
  readonly endsInBlankLine: string;
  readonly followUp: string;
  readonly intoDocs: string;
  readonly mixed: string;
  readonly quotedName: string;
  readonly released: string;
  readonly unmerged: string;
}

/**
 * A release tag, its documentation branch, documentation and code commits after it, and a fresh
 * clone of them.
 */
interface Fixture {
  readonly clone: string;
  readonly commits: FixtureCommits;
  readonly origin: string;
}

function createFixture(): Fixture {
  const root = mkdtempSync(path.join(TEMPORARY_ROOT, "case-"));
  const origin = path.join(root, "origin.git");
  const work = path.join(root, "work");
  const clone = path.join(root, "clone");
  git(root, "init", "--quiet", "--bare", "--initial-branch=main", origin);
  git(root, "clone", "--quiet", origin, work);

  const released = commitFiles(work, "Release", {
    [GUIDE_PATH]: "first line\nsecond line\n",
    [SPACED_PATH]: "rule\n\n",
    "src/lib.rs": "fn released() {}\n",
    "src/notes.txt": "maintainer notes\n",
  });
  git(work, "tag", RELEASE_TAG);
  const docs = commitFiles(work, "Fix guide typo", { [GUIDE_PATH]: "first line, fixed\nsecond line\n" });
  const code = commitFiles(work, "Add feature", { "src/lib.rs": "fn unreleased() {}\n" });
  commitFiles(work, "Add feature and its docs", {
    [GUIDE_PATH]: "first line, fixed\nsecond line\nunreleased feature\n",
    "src/lib.rs": "fn unreleased() {}\nfn more() {}\n",
  });
  const conflicting = commitFiles(work, "Reword guide", {
    [GUIDE_PATH]: "first line, fixed\nsecond line\nunreleased feature, reworded\n",
  });
  const followUp = commitFiles(work, "Add FAQ", { "site/src/content/docs/faq.md": "answers\n" });
  git(work, "mv", "src/notes.txt", "site/src/content/docs/notes.txt");
  git(work, "commit", "--quiet", "--message", "Move notes into the docs");
  const intoDocs = git(work, "rev-parse", "HEAD");
  const quotedName = commitFiles(work, "Add café guide", { "site/src/content/docs/café.md": "coffee\n" });
  const asset = commitFiles(work, "Add illustration", { "site/src/assets/tree.svg": "<svg/>\n" });
  const mixed = commitFiles(work, "Add preview image and its tooling", {
    "site/astro.config.ts": "export default {};\n",
    "site/public/card.png": "png\n",
    "src/lib.rs": "fn unreleased() {}\nfn more() {}\nfn card() {}\n",
  });
  // The patch for this change ends with the unchanged blank line, which a diff writes as a space.
  const endsInBlankLine = commitFiles(work, "Reword rule", { [SPACED_PATH]: "rule, reworded\n\n" });
  git(work, "switch", "--quiet", "--create", "feature", released);
  const unmerged = commitFiles(work, "Draft guide", { "site/src/content/docs/draft.md": "draft\n" });
  git(work, "push", "--quiet", "origin", "main", "feature", RELEASE_TAG, `${released}:${DOCS_BRANCH}`);
  git(root, "clone", "--quiet", origin, clone);

  return {
    clone,
    commits: { asset, code, conflicting, docs, endsInBlankLine, followUp, intoDocs, mixed, quotedName, released, unmerged },
    origin,
  };
}

function apply(fixture: Fixture, docsCommit: string) {
  return applyDocsFix({ docsCommit, releaseTag: RELEASE_TAG, repository: fixture.clone });
}

function originBranch(fixture: Fixture): string | null {
  const heads = git(fixture.origin, "for-each-ref", "--format=%(objectname)", DOCS_BRANCH);
  return heads.length === 0 ? null : heads;
}

describe("applyDocsFix", () => {
  describe("when the documentation branch does not exist", () => {
    it("rejects the commit without creating the branch", () => {
      const fixture = createFixture();
      git(fixture.origin, "update-ref", "-d", DOCS_BRANCH);

      const result = apply(fixture, fixture.commits.docs);

      assert.deepEqual(result, { kind: "docs_branch_missing", releaseTag: RELEASE_TAG });
      assert.equal(originBranch(fixture), null);
    });
  });

  describe("when the documentation branch does not descend from the release tag", () => {
    it("rejects the commit and leaves the branch as it is", () => {
      const fixture = createFixture();
      const unrelated = git(fixture.origin, "commit-tree", "-m", "Same files, other history", `${RELEASE_TAG}^{tree}`);
      git(fixture.origin, "update-ref", DOCS_BRANCH, unrelated);

      const result = apply(fixture, fixture.commits.docs);

      assert.deepEqual(result, {
        kind: "docs_branch_not_from_release",
        branchHead: unrelated,
        releaseTag: RELEASE_TAG,
      });
      assert.equal(originBranch(fixture), unrelated);
    });
  });

  describe("when the documentation branch is at the release tag", () => {
    describe("and the commit changes only documentation", () => {
      it("adds the commit on top of the release", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.docs);

        assert.ok(result.kind === "applied");
        assert.equal(result.releaseVersion, "1.2.3");
        assert.equal(originBranch(fixture), result.commit);
        assert.equal(git(fixture.origin, "rev-parse", `${result.commit}^`), fixture.commits.released);
        assert.equal(
          git(fixture.origin, "show", `${result.commit}:${GUIDE_PATH}`),
          "first line, fixed\nsecond line",
        );
        assert.match(
          git(fixture.origin, "log", "-1", "--format=%B", result.commit),
          new RegExp(`\\(cherry picked from commit ${fixture.commits.docs}\\)`, "u"),
        );
      });
    });

    describe("and the commit changes only a documentation asset", () => {
      it("adds the commit on top of the release", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.asset);

        assert.ok(result.kind === "applied");
        assert.equal(originBranch(fixture), result.commit);
      });
    });

    describe("and the commit's change ends before an unchanged blank line", () => {
      it("applies the change with the blank line kept", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.endsInBlankLine);

        assert.ok(result.kind === "applied");
        assert.equal(
          git(fixture.origin, "cat-file", "-s", `${result.commit}:${SPACED_PATH}`),
          String("rule, reworded\n\n".length),
        );
      });
    });

    describe("and the commit changes the site's configuration and static files along with code", () => {
      it("adds only the site's changes", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.mixed);

        assert.ok(result.kind === "applied");
        assert.equal(originBranch(fixture), result.commit);
        assert.equal(
          git(fixture.origin, "diff", "--name-only", RELEASE_TAG, result.commit),
          "site/astro.config.ts\nsite/public/card.png",
        );
        assert.equal(
          git(fixture.origin, "log", "-1", "--format=%an <%ae>", result.commit),
          "Test <test@example.com>",
        );
      });
    });

    describe("and the script runs from a subdirectory of the checkout", () => {
      it("still applies the site's changes", () => {
        const fixture = createFixture();

        const result = applyDocsFix({
          docsCommit: fixture.commits.mixed,
          releaseTag: RELEASE_TAG,
          repository: path.join(fixture.clone, "src"),
        });

        assert.ok(result.kind === "applied");
        assert.equal(
          git(fixture.origin, "diff", "--name-only", RELEASE_TAG, result.commit),
          "site/astro.config.ts\nsite/public/card.png",
        );
      });
    });

    describe("and the commit changes only code", () => {
      it("rejects it and leaves the branch as it is", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.code);

        assert.deepEqual(result, { kind: "no_documentation_changes", docsCommit: fixture.commits.code });
        assert.equal(originBranch(fixture), fixture.commits.released);
      });
    });

    describe("and the commit moves a file into the documentation", () => {
      it("adds the file and keeps the original outside the documentation", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.intoDocs);

        assert.ok(result.kind === "applied");
        assert.equal(git(fixture.origin, "diff", "--name-only", RELEASE_TAG, result.commit), "site/src/content/docs/notes.txt");
      });
    });

    describe("and the commit adds a documentation file whose name Git quotes", () => {
      it("applies it", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.quotedName);

        assert.equal(result.kind, "applied");
      });
    });

    describe("and the commit is already part of the release", () => {
      it("rejects it", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.released);

        assert.deepEqual(result, { kind: "docs_commit_released", docsCommit: fixture.commits.released });
      });
    });

    describe("and the commit is not on main", () => {
      it("rejects it", () => {
        const fixture = createFixture();

        const result = apply(fixture, fixture.commits.unmerged);

        assert.deepEqual(result, { kind: "docs_commit_not_on_main", docsCommit: fixture.commits.unmerged });
        assert.equal(originBranch(fixture), fixture.commits.released);
      });
    });
  });

  describe("when the documentation branch has an earlier fix", () => {
    it("adds the commit on top of that fix", () => {
      const fixture = createFixture();
      const first = apply(fixture, fixture.commits.docs);

      const second = apply(fixture, fixture.commits.followUp);

      assert.ok(first.kind === "applied" && second.kind === "applied");
      assert.equal(originBranch(fixture), second.commit);
      assert.equal(git(fixture.origin, "rev-parse", `${second.commit}^`), first.commit);
      assert.equal(git(fixture.origin, "show", `${second.commit}:site/src/content/docs/faq.md`), "answers");
    });
  });

  describe("when the documentation branch already has the commit", () => {
    it("leaves the branch as it is", () => {
      const fixture = createFixture();
      const first = apply(fixture, fixture.commits.docs);

      const second = apply(fixture, fixture.commits.docs);

      assert.ok(first.kind === "applied");
      assert.deepEqual(second, { ...first, kind: "already_applied" });
      assert.equal(originBranch(fixture), first.commit);
    });
  });

  describe("when the commit does not apply to the documentation branch", () => {
    it("rejects it and leaves the branch as it is", () => {
      const fixture = createFixture();
      const first = apply(fixture, fixture.commits.docs);

      const result = apply(fixture, fixture.commits.conflicting);

      assert.equal(result.kind, "cherry_pick_failed");
      assert.ok(first.kind === "applied");
      assert.equal(originBranch(fixture), first.commit);
      assert.equal(git(fixture.clone, "status", "--porcelain"), "");
    });
  });

  describe("when the documentation branch contains a change outside the documentation", () => {
    it("rejects the branch", () => {
      const fixture = createFixture();
      const outside = path.join(fixture.clone, "..", "outside");
      git(fixture.clone, "worktree", "add", "--quiet", "--detach", outside, RELEASE_TAG);
      commitFiles(outside, "Change code", { "src/lib.rs": "fn changed() {}\n" });
      git(outside, "push", "--quiet", "origin", `HEAD:${DOCS_BRANCH}`);
      const tampered = originBranch(fixture);

      const result = apply(fixture, fixture.commits.docs);

      assert.deepEqual(result, { kind: "non_documentation_paths", paths: ["src/lib.rs"] });
      assert.equal(originBranch(fixture), tampered);
    });
  });

  describe("when the inputs are malformed", () => {
    it("rejects an abbreviated commit", () => {
      const fixture = createFixture();

      const result = apply(fixture, fixture.commits.docs.slice(0, 12));

      assert.deepEqual(result, { kind: "invalid_docs_commit", docsCommit: fixture.commits.docs.slice(0, 12) });
    });

    it("rejects a tag that is not a release tag", () => {
      const fixture = createFixture();

      const result = applyDocsFix({
        docsCommit: fixture.commits.docs,
        releaseTag: "latest",
        repository: fixture.clone,
      });

      assert.deepEqual(result, { kind: "invalid_release_tag", releaseTag: "latest" });
    });
  });
});
