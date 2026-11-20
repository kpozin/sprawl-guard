import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { markdownToHtml, markdownToMdast, mdxToMdast } from "satteri";

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

  describe("when the text has a command as inline code", () => {
    describe("and the command is in prose", () => {
      test("shows the command as written and hides each installation's until a tab is picked", () => {
        assert.equal(
          markdownToHtml("Run `sprawl-guard check`.", options).html,
          '<p>Run <span class="synced-variants" data-sync-key="package-manager">' +
            "<span><code>sprawl-guard check</code></span>" +
            '<span data-sync-label="pnpm" hidden data-pagefind-ignore><code>pnpm exec sprawl-guard check</code></span>' +
            '<span data-sync-label="npm" hidden data-pagefind-ignore><code>npm exec -- @kpozin/sprawl-guard check</code></span>' +
            '<span data-sync-label="uv" hidden data-pagefind-ignore><code>uv run sprawl-guard check</code></span>' +
            '<span data-sync-label="Cargo" hidden data-pagefind-ignore><code>sprawl-guard check</code></span>' +
            '<span data-sync-label="Homebrew" hidden data-pagefind-ignore><code>sprawl-guard check</code></span>' +
            "</span>.</p>\n",
        );
      });
    });

    describe("and the command is inside a package-manager tab", () => {
      test("shows only that tab's command", () => {
        const tree = mdxToMdast(
          '<Tabs syncKey="package-manager"><TabItem label="uv">\n\nRun `sprawl-guard check`.\n\n</TabItem></Tabs>',
          options,
        );
        assert.match(
          JSON.stringify(tree),
          /"type":"inlineCode","value":"uv run sprawl-guard check"/u,
        );
        assert.doesNotMatch(JSON.stringify(tree), /syncedVariants/u);
      });
    });

    describe("and the command is in a heading", () => {
      test("keeps the command as written, so the heading has one anchor", () => {
        const source = "## `sprawl-guard check`";
        assert.deepEqual(markdownToMdast(source, options), markdownToMdast(source));
      });
    });

    describe("and the code names the program without arguments", () => {
      test("keeps the name as written", () => {
        const source = "Install `sprawl-guard`.";
        assert.deepEqual(markdownToMdast(source, options), markdownToMdast(source));
      });
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
