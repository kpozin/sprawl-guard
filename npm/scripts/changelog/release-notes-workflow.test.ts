import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { parse } from "yaml";
import { z } from "zod";

/** The public repository root, where the release workflow's commands run. */
const PUBLIC_ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const RELEASE_WORKFLOW_PATH = path.join(PUBLIC_ROOT, ".github/workflows/release.yml");
const DRAFT_STEP_NAME = "Create or update draft GitHub Release";
/** How long the step may run before the test fails instead of hanging the suite. */
const STEP_TIMEOUT_MS = 30_000;

/** The part of the release workflow this test runs. */
const RELEASE_WORKFLOW_SCHEMA = z.object({
  jobs: z.object({
    "draft-release": z.object({
      steps: z.array(z.object({ name: z.string(), run: z.string().optional() })),
    }),
  }),
});

describe("the release workflow's draft step", () => {
  describe("for a tag with no release yet", () => {
    const directory = mkdtempSync(path.join(tmpdir(), "release-notes-"));
    after(() => rmSync(directory, { force: true, recursive: true }));

    it("uses the version's changelog section as the release notes", () => {
      const command = RELEASE_WORKFLOW_SCHEMA.parse(
        parse(readFileSync(RELEASE_WORKFLOW_PATH, "utf8")),
      ).jobs["draft-release"].steps.find(({ name }) => name === DRAFT_STEP_NAME)?.run;
      assert.ok(command !== undefined);
      const recordedNotes = path.join(directory, "recorded-notes.md");
      // Reports no existing release, and records the notes file that `release create` gets.
      writeFileSync(
        path.join(directory, "gh"),
        [
          "#!/usr/bin/env bash",
          "set -euo pipefail",
          '[[ "$2" == view ]] && exit 1',
          'while (( $# > 1 )) && [[ "$1" != --notes-file ]]; do shift; done',
          '[[ "$1" == --notes-file ]] || { echo "gh stub: no --notes-file" >&2; exit 2; }',
          'cp "$2" "${RECORDED_NOTES}"',
        ].join("\n"),
        { mode: 0o755 },
      );

      const result = spawnSync("bash", ["-euo", "pipefail", "-c", command], {
        cwd: PUBLIC_ROOT,
        encoding: "utf8",
        timeout: STEP_TIMEOUT_MS,
        env: {
          GITHUB_LATEST: "true",
          GITHUB_PRERELEASE: "false",
          PATH: `${directory}${path.delimiter}${process.env["PATH"]}`,
          RECORDED_NOTES: recordedNotes,
          RELEASE_TAG: "v0.5.0",
          RELEASE_VERSION: "0.5.0",
          RUNNER_TEMP: directory,
        },
      });

      assert.equal(result.status, 0, result.stderr);
      assert.match(readFileSync(recordedNotes, "utf8"), /^### Added\n\n- `check --format github`/u);
    });
  });
});
