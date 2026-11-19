import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { markdownToMdast, mdxToMdast } from "satteri";

import { cliCommandTabsPlugin } from "./cli-command-tabs.ts";

const options = { mdastPlugins: [cliCommandTabsPlugin] };

/** The tabs that the plugin builds for one block, given each installation's commands. */
function tabs(meta: string | null, commands: readonly [string, string, string, string, string]) {
  return {
    type: "mdxJsxFlowElement",
    name: "SprawlGuardCliTabs",
    attributes: [{ type: "mdxJsxAttribute", name: "syncKey", value: "package-manager" }],
    children: (["pnpm", "npm", "uv", "Cargo", "Homebrew"] as const).map((label, index) => ({
      type: "mdxJsxFlowElement",
      name: "SprawlGuardCliTabItem",
      attributes: [{ type: "mdxJsxAttribute", name: "label", value: label }],
      children: [{ type: "code", lang: "sh", meta, value: commands[index] }],
    })),
  };
}

describe("CLI command tabs", () => {
  describe("when an MDX page has flagged command blocks", () => {
    test("imports the tab components once and shows each block as one tab per installation", () => {
      const tree = mdxToMdast(
        '```sh sprawl-guard title="Check"\nsprawl-guard check\n```\n\n```sh sprawl-guard\nsprawl-guard update\n```',
        options,
      );
      assert.deepEqual(tree.children, [
        {
          type: "mdxjsEsm",
          value:
            'import { Tabs as SprawlGuardCliTabs, TabItem as SprawlGuardCliTabItem } from "@astrojs/starlight/components";',
        },
        tabs('title="Check"', [
          "pnpm exec sprawl-guard check",
          "npm exec -- @kpozin/sprawl-guard check",
          "uv run sprawl-guard check",
          "sprawl-guard check",
          "sprawl-guard check",
        ]),
        tabs(null, [
          "pnpm exec sprawl-guard update",
          "npm exec -- @kpozin/sprawl-guard update",
          "uv run sprawl-guard update",
          "sprawl-guard update",
          "sprawl-guard update",
        ]),
      ]);
    });
  });

  describe("when an MDX page has no flagged command block", () => {
    test("leaves the page as it is", () => {
      const source = "```sh\ncargo install sprawl-guard\n```";
      assert.deepEqual(mdxToMdast(source, options), mdxToMdast(source));
    });
  });

  describe("when a plain Markdown page has a flagged command block", () => {
    test("fails, because only MDX pages can hold tabs", () => {
      assert.throws(
        () => markdownToMdast("```sh sprawl-guard\nsprawl-guard check\n```", options),
        /needs an \.mdx page/u,
      );
    });
  });
});
