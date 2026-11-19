import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { INSTALLATIONS, commandsFor, parseCodeBlock } from "./cli-commands.ts";

describe("command block parsing", () => {
  describe("when a shell block is flagged", () => {
    describe("and every line runs Sprawl Guard", () => {
      const parsed = parseCodeBlock({
        lang: "sh",
        meta: 'sprawl-guard title="Initialize a repository"',
        value: "sprawl-guard init\nsprawl-guard init --write",
      });

      test("keeps the rest of the meta and each line's arguments", () => {
        assert.deepEqual(parsed, {
          ok: true,
          value: {
            kind: "cli_commands",
            block: {
              meta: 'title="Initialize a repository"',
              argumentLines: [" init", " init --write"],
            },
          },
        });
      });

      test("writes the commands for each installation", () => {
        assert.ok(parsed.ok && parsed.value.kind === "cli_commands");
        const block = parsed.value.block;
        assert.deepEqual(
          INSTALLATIONS.map((installation) => commandsFor(block, installation)),
          [
            "pnpm exec sprawl-guard init\npnpm exec sprawl-guard init --write",
            "npm exec -- @kpozin/sprawl-guard init\nnpm exec -- @kpozin/sprawl-guard init --write",
            "uv run sprawl-guard init\nuv run sprawl-guard init --write",
            "sprawl-guard init\nsprawl-guard init --write",
            "sprawl-guard init\nsprawl-guard init --write",
          ],
        );
      });
    });

    describe("and the flag is the whole meta", () => {
      test("has no remaining meta", () => {
        assert.deepEqual(
          parseCodeBlock({ lang: "sh", meta: "sprawl-guard", value: "sprawl-guard" }),
          {
            ok: true,
            value: { kind: "cli_commands", block: { meta: null, argumentLines: [""] } },
          },
        );
      });
    });

    describe("and a line runs another program", () => {
      test("fails with that line", () => {
        assert.deepEqual(
          parseCodeBlock({
            lang: "sh",
            meta: "sprawl-guard",
            value: "sprawl-guard check\nsprawl-guardian",
          }),
          { ok: false, error: { kind: "not_a_command", line: "sprawl-guardian" } },
        );
      });
    });
  });

  describe("when a block in another language is flagged", () => {
    test("fails with the language", () => {
      assert.deepEqual(
        parseCodeBlock({ lang: "bash", meta: "sprawl-guard", value: "sprawl-guard check" }),
        {
          ok: false,
          error: { kind: "not_a_shell_block", lang: "bash" },
        },
      );
    });
  });

  describe("when a block isn't flagged", () => {
    describe("and it is a shell block whose every line runs Sprawl Guard", () => {
      test("reports the missing flag", () => {
        assert.deepEqual(
          parseCodeBlock({ lang: "sh", meta: 'title="Check"', value: "sprawl-guard check" }),
          {
            ok: true,
            value: { kind: "unflagged_cli_commands" },
          },
        );
      });
    });

    describe("and it runs other programs", () => {
      test("is an ordinary block", () => {
        assert.deepEqual(
          parseCodeBlock({
            lang: "sh",
            meta: null,
            value: "cargo install sprawl-guard\nsprawl-guard check",
          }),
          { ok: true, value: { kind: "other" } },
        );
      });
    });

    describe("and the flag appears later in the meta", () => {
      test("is an ordinary block", () => {
        assert.deepEqual(
          parseCodeBlock({ lang: "toml", meta: 'title="sprawl-guard"', value: "" }),
          {
            ok: true,
            value: { kind: "other" },
          },
        );
      });
    });
  });
});
