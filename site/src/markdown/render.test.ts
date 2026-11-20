import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { gfmToMarkdown } from "mdast-util-gfm";
import { toMarkdown } from "mdast-util-to-markdown";

import { cliCommandReferenceNodes } from "./cli-reference-nodes.ts";
import { renderDocumentationMarkdown } from "./render.ts";
import { createDocumentationRoutes } from "./routes.ts";

const routeResult = createDocumentationRoutes([
  "index",
  "quick-start",
  "reference/cli-and-output",
  "run-in-ci",
]);
assert.equal(routeResult.ok, true);
if (!routeResult.ok) throw new Error("test route setup failed");
const routes = routeResult.value;

describe("documentation Markdown rendering", () => {
  describe("when an entry uses the supported Starlight components", () => {
    test("preserves every tab and the ordered steps", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "reference/cli-and-output",
          title: "Quick start",
          description: "Install and run the first check.",
          body: `import { Steps, TabItem, Tabs } from "@astrojs/starlight/components";

### Installation commands

<Tabs syncKey="package-manager">
  <TabItem label="npm">

\`\`\`sh
npm exec -- @kpozin/sprawl-guard check
\`\`\`

  </TabItem>
  <TabItem label="pnpm">

\`\`\`sh
pnpm exec sprawl-guard check
\`\`\`

  </TabItem>
</Tabs>

<Steps>

1. Inspect the repository.
2. [Run in CI](/sprawl-guard/run-in-ci/).

</Steps>`,
        },
        routes,
      );

      assert.equal(result.ok, true);
      if (!result.ok) return;
      assert.match(result.value, /^# Quick start\n\nInstall and run the first check\./u);
      assert.match(result.value, /^### Installation commands$/mu);
      assert.match(
        result.value,
        /^- \*\*npm\*\*\n\n {2}```sh\n {2}npm exec -- @kpozin\/sprawl-guard check\n {2}```$/mu,
      );
      assert.match(
        result.value,
        /^- \*\*pnpm\*\*\n\n {2}```sh\n {2}pnpm exec sprawl-guard check/mu,
      );
      assert.match(result.value, /1\. Inspect the repository\./u);
      assert.match(result.value, /2\. \[Run in CI\]\(\/sprawl-guard\/run-in-ci\.md\)\./u);
    });

    test("rewrites reference-style documentation links", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "quick-start",
          title: "Quick start",
          description: "Reference link example.",
          body: "[Run it in CI][ci].\n\n[ci]: /sprawl-guard/run-in-ci/#workflow",
        },
        routes,
      );

      assert.equal(result.ok, true);
      if (!result.ok) return;
      assert.match(result.value, /\[ci\]: \/sprawl-guard\/run-in-ci\.md#workflow/u);
    });
  });

  describe("when an entry renders generated CLI reference data", () => {
    test("includes usage, arguments, conflicts, and allowed values", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "reference/cli-and-output",
          title: "CLI",
          description: "Command reference.",
          body: `import CliCommandReference from "../../../components/CliCommandReference.astro";

<CliCommandReference path="sprawl-guard check" />`,
        },
        routes,
      );

      assert.equal(result.ok, true);
      if (!result.ok) return;
      assert.match(result.value, /`Usage: sprawl-guard check \[OPTIONS\] \[PATHS\]\.\.\.`/u);
      assert.match(result.value, /Argument or option\s+\| Meaning/u);
      assert.match(result.value, /`--require-ratchet-current`/u);
      assert.match(result.value, /Conflicts with:[\s\S]*`--no-require-ratchet-current`/u);
      assert.match(result.value, /Allowed values:[\s\S]*`sarif`/u);
    });

    test("preserves inline code in allowed-value help", () => {
      const markdown = toMarkdown(
        {
          type: "root",
          children: cliCommandReferenceNodes({
            path: ["sprawl-guard", "example"],
            usage: "Usage: sprawl-guard example",
            arguments: [
              {
                id: "format",
                syntax: "--format <FORMAT>",
                help: "Output format",
                conflicts_with: [],
                default_values: [],
                possible_values: [{ name: "json", help: "Writes `stdout` as JSON" }],
              },
            ],
          }),
        },
        { extensions: [gfmToMarkdown()] },
      );

      assert.match(markdown, /`json` — Writes `stdout` as JSON/u);
    });
  });

  describe("when the homepage has a Starlight hero", () => {
    test("preserves the tagline's link to its fine print", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "index",
          title: "Sprawl Guard",
          description: "Keep source trees navigable.",
          body: `<a id="supported-languages"></a>Supported languages. Run \`sprawl-guard languages\`.`,
          hero: {
            tagline:
              'Any<a href="#supported-languages" aria-label="Supported languages">*</a> language.',
            actions: [],
          },
        },
        routes,
      );

      assert.equal(result.ok, true);
      if (!result.ok) return;
      assert.match(
        result.value,
        /Any<a href="#supported-languages" aria-label="Supported languages">\*<\/a> language\./u,
      );
      assert.match(
        result.value,
        /<a id="supported-languages"><\/a>Supported languages\. Run `sprawl-guard languages`\./u,
      );
    });

    test("includes its text and actions but not the description or its image", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "index",
          title: "Sprawl Guard",
          description: "Keep source trees navigable.",
          body: "Body.",
          hero: {
            tagline: "Keep changes focused.",
            actions: [{ text: "Get started", link: "./quick-start/" }],
          },
        },
        routes,
      );

      assert.equal(result.ok, true);
      if (!result.ok) return;
      assert.match(result.value, /Keep changes focused\./u);
      assert.match(result.value, /\[Get started\]\(\/sprawl-guard\/quick-start\.md\)/u);
      assert.doesNotMatch(result.value, /image/u);
      assert.doesNotMatch(result.value, /navigable/u);
    });
  });

  describe("when the homepage shows the directory tree illustration", () => {
    test("keeps the text beside the illustration and leaves the illustration out", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "index",
          title: "Sprawl Guard",
          description: "Keep source trees navigable.",
          body: `import DirectoryTrees from "../../components/DirectoryTrees.astro";

<DirectoryTrees>

Beside.

</DirectoryTrees>

After.`,
        },
        routes,
      );

      assert.deepEqual(result, {
        ok: true,
        value: "# Sprawl Guard\n\nKeep source trees navigable.\n\nBeside.\n\nAfter.\n",
      });
    });
  });

  describe("when an entry has a collapsed section", () => {
    const render = (body: string) =>
      renderDocumentationMarkdown(
        { id: "quick-start", title: "Start", description: "Install.", body },
        routes,
      );

    describe("and it opens with a summary", () => {
      test("shows the summary in bold followed by the content", () => {
        assert.deepEqual(
          render("<details>\n<summary>Supported systems</summary>\n\n- macOS\n\n</details>"),
          { ok: true, value: "# Start\n\nInstall.\n\n**Supported systems**\n\n- macOS\n" },
        );
      });
    });

    describe("and it has no summary", () => {
      test("fails", () => {
        const result = render("<details>\n\n- macOS\n\n</details>");
        assert.equal(result.ok, false);
      });
    });

    describe("and its summary is empty", () => {
      test("fails", () => {
        const result = render("<details>\n<summary></summary>\n\n- macOS\n\n</details>");
        assert.equal(result.ok, false);
      });
    });

    describe("and its summary has paragraphs", () => {
      test("fails", () => {
        const result = render(
          "<details>\n<summary>\n\nSupported systems\n\nOther details\n\n</summary>\n\n- macOS\n\n</details>",
        );
        assert.equal(result.ok, false);
      });
    });
  });

  describe("when an entry contains an unsupported component", () => {
    test("rejects executable attributes on an anchor target", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "index",
          title: "Sprawl Guard",
          description: "Keep source trees navigable.",
          body: '<a id="note" onClick="run()"></a>Note.',
        },
        routes,
      );

      assert.equal(result.ok, false);
      if (result.ok) return;
      assert.equal(result.error.kind, "malformed_mdx_component");
    });

    test("fails without omitting the component", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "quick-start",
          title: "Unsupported",
          description: "Unsupported component example.",
          body: "<Widget />",
        },
        routes,
      );

      assert.deepEqual(result, {
        ok: false,
        error: {
          kind: "unsupported_mdx_construct",
          entryId: "quick-start",
          construct: "Widget",
        },
      });
    });

    test("rejects a supported component name imported from another module", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "quick-start",
          title: "Unsupported",
          description: "Wrong import example.",
          body: `import Tabs from "./DifferentTabs.astro";
import { TabItem } from "@astrojs/starlight/components";

<Tabs><TabItem label="npm">content</TabItem></Tabs>`,
        },
        routes,
      );

      assert.deepEqual(result, {
        ok: false,
        error: {
          kind: "unsupported_mdx_construct",
          entryId: "quick-start",
          construct: "Tabs imported from ./DifferentTabs.astro",
        },
      });
    });
  });

  describe("when a supported component is malformed", () => {
    test("returns the component and validation failure", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "quick-start",
          title: "Malformed",
          description: "Malformed component example.",
          body: 'import { Steps } from "@astrojs/starlight/components";\n\n<Steps label="Unexpected">\n\n1. First\n\n</Steps>',
        },
        routes,
      );

      assert.deepEqual(result, {
        ok: false,
        error: {
          kind: "malformed_mdx_component",
          entryId: "quick-start",
          component: "Steps",
          detail: "attributes are not supported",
        },
      });
    });
  });

  describe("when an entry contains executable MDX", () => {
    test("rejects an expression", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "quick-start",
          title: "Expression",
          description: "Expression example.",
          body: "Value: {value}",
        },
        routes,
      );

      assert.equal(result.ok, false);
      if (result.ok) return;
      assert.deepEqual(result.error, {
        kind: "unsupported_mdx_construct",
        entryId: "quick-start",
        construct: "MDX expression",
      });
    });

    test("rejects an export", () => {
      const result = renderDocumentationMarkdown(
        {
          id: "quick-start",
          title: "Export",
          description: "Export example.",
          body: "export const value = 1;",
        },
        routes,
      );

      assert.deepEqual(result, {
        ok: false,
        error: {
          kind: "unsupported_mdx_construct",
          entryId: "quick-start",
          construct: "MDX export",
        },
      });
    });
  });
  describe("when an entry has command blocks", () => {
    /** Renders a quick-start entry whose body is `body`. */
    const render = (body: string) =>
      renderDocumentationMarkdown(
        { id: "quick-start", title: "Commands", description: "Command example.", body },
        routes,
      );

    describe("and flagged blocks are nested in a list", () => {
      const result = render(
        '1. Check.\n\n   ```sh sprawl-guard title="Check"\n   sprawl-guard check\n   ```\n\n2. Update.\n\n   ```sh sprawl-guard\n   sprawl-guard update\n   ```',
      );

      test("keeps the commands and title without the flag", () => {
        assert.ok(result.ok);
        assert.match(result.value, /^ {3}```sh title="Check"\n {3}sprawl-guard check\n {3}```$/mu);
        assert.match(result.value, /^ {3}```sh\n {3}sprawl-guard update\n {3}```$/mu);
      });

      test("explains once how to run the commands through a package manager", () => {
        assert.ok(result.ok);
        const note =
          "Commands on this page run `sprawl-guard` as installed with Cargo or Homebrew. If you installed Sprawl Guard with a package manager, run it this way instead:\n\n- pnpm: `pnpm exec sprawl-guard`\n- npm: `npm exec -- @kpozin/sprawl-guard`\n- uv: `uv run sprawl-guard`\n";
        assert.equal(result.value.split(note).length - 1, 1);
      });
    });

    describe("and no block is flagged", () => {
      describe("but the text has an inline command", () => {
        test("keeps the command as written and adds the package-manager note", () => {
          const result = render("Run `sprawl-guard check`.");
          assert.ok(result.ok);
          assert.match(result.value, /Run `sprawl-guard check`\./u);
          assert.match(result.value, /Commands on this page/u);
        });
      });

      describe("and the text names the program only", () => {
        test("leaves out the package-manager note", () => {
          const result = render(
            "Install `sprawl-guard`.\n\n```sh\ncargo install sprawl-guard\n```",
          );
          assert.ok(result.ok);
          assert.doesNotMatch(result.value, /Commands on this page/u);
        });
      });
    });

    describe("and a block runs only Sprawl Guard without the flag", () => {
      test("fails so that the HTML site doesn't lose the block's tabs", () => {
        assert.deepEqual(render("```sh\nsprawl-guard check\n```"), {
          ok: false,
          error: {
            kind: "unflagged_cli_command_block",
            entryId: "quick-start",
            value: "sprawl-guard check",
          },
        });
      });
    });

    describe("and a flagged block runs another program", () => {
      test("fails with the line that isn't a command", () => {
        assert.deepEqual(render("```sh sprawl-guard\ncargo install sprawl-guard\n```"), {
          ok: false,
          error: {
            kind: "cli_command_block",
            entryId: "quick-start",
            cause: { kind: "not_a_command", line: "cargo install sprawl-guard" },
          },
        });
      });
    });
  });
});
