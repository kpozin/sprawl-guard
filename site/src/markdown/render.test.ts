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
npm exec -- sprawl-guard check
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
      assert.match(result.value, /\*\*npm\*\*[\s\S]*npm exec -- sprawl-guard check/u);
      assert.match(result.value, /\*\*pnpm\*\*[\s\S]*pnpm exec sprawl-guard check/u);
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
    test("includes its text and actions but not its image", () => {
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

  describe("when an entry contains an unsupported component", () => {
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
});
