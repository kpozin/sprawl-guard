import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import type { AstroIntegration } from "astro";

import type { DocumentationRoutes } from "../markdown/routes.ts";
import { documentationRoutesFromPagePathnames } from "./markdown-output.ts";

const SITE_BASE_PATH = "/sprawl-guard";

/** Verifies the generated Markdown counterparts after Astro finishes the static build. */
export default function markdownOutputCheck(): AstroIntegration {
  return {
    name: "sprawl-guard-markdown-output-check",
    hooks: {
      "astro:build:done": async ({ dir, pages }) => {
        const routes = documentationRoutesFromPagePathnames(pages.map((page) => page.pathname));
        if (!routes.ok) {
          throw new Error(`Cannot discover built documentation pages: ${routes.error.kind}`);
        }
        await verifyMarkdownOutput(dir, routes.value);
      },
    },
  };
}

async function verifyMarkdownOutput(
  outputDirectory: URL,
  routes: DocumentationRoutes,
): Promise<void> {
  const expectedMarkdownPaths = new Set(
    routes.entries.map((route) => route.markdownPath.slice(`${SITE_BASE_PATH}/`.length)),
  );
  const expectedHtmlPaths = new Set(routes.entries.map((route) => route.htmlPath));

  for (const route of routes.entries) {
    const markdownRelativePath = route.markdownPath.slice(`${SITE_BASE_PATH}/`.length);
    const htmlRelativePath =
      route.entryId === "index" ? "index.html" : `${route.entryId}/index.html`;
    const [markdown, html] = await Promise.all([
      readOutput(outputDirectory, markdownRelativePath),
      readOutput(outputDirectory, htmlRelativePath),
    ]);

    assert.deepEqual(
      markdownAlternateHrefs(html),
      [route.markdownPath],
      `${htmlRelativePath} must advertise its ${markdownRelativePath} alternate exactly once`,
    );
    assert.match(markdown, /^# .+/u, `${markdownRelativePath} must begin with its title`);
    assert.doesNotMatch(
      markdown,
      /<(?:CliCommandReference|Steps|TabItem|Tabs)\b/u,
      `${markdownRelativePath} contains an unrendered MDX component`,
    );
    assert.doesNotMatch(markdown, /^import\s/mu, `${markdownRelativePath} contains an MDX import`);
    verifyMarkdownLinks(markdownRelativePath, markdown, expectedMarkdownPaths, expectedHtmlPaths);
  }

  const fallbackHtml = await readOutput(outputDirectory, "404.html");
  assert.doesNotMatch(
    fallbackHtml,
    /<link rel="alternate" type="text\/markdown"/u,
    "404.html must not advertise a Markdown alternate",
  );
  await assert.rejects(readOutput(outputDirectory, "404.md"), /ENOENT/u);

  const quickStart = await readOutput(outputDirectory, "quick-start.md");
  for (const packageManager of ["Cargo", "npm", "pnpm"]) {
    assert.match(quickStart, new RegExp(`\\*\\*${packageManager}\\*\\*`, "u"));
  }
  assert.match(
    quickStart,
    /sprawl-guard init[\s\S]*sprawl-guard init --write[\s\S]*sprawl-guard languages --detected[\s\S]*sprawl-guard check/u,
  );

  const cliReference = await readOutput(outputDirectory, "reference/cli-and-output.md");
  assert.match(cliReference, /`Usage: sprawl-guard <COMMAND>`/u);
  assert.match(cliReference, /Argument or option\s+\| Meaning/u);
  assert.match(cliReference, /`-V, --version`/u);
  assert.match(cliReference, /\*\*Allowed values:\*\*/u);
  assert.match(cliReference, /\*\*Conflicts with:\*\*/u);
}

async function readOutput(outputDirectory: URL, relativePath: string): Promise<string> {
  return readFile(new URL(relativePath, outputDirectory), "utf8");
}

function markdownAlternateHrefs(html: string): string[] {
  return [...html.matchAll(/<link\b[^>]*>/gu)]
    .map((match) => match[0])
    .filter((tag) => /\brel="alternate"/u.test(tag) && /\btype="text\/markdown"/u.test(tag))
    .flatMap((tag) => {
      const href = /\bhref="([^"]+)"/u.exec(tag);
      return href === null ? [] : [href[1]];
    });
}

function verifyMarkdownLinks(
  sourcePath: string,
  markdown: string,
  expectedMarkdownPaths: ReadonlySet<string>,
  expectedHtmlPaths: ReadonlySet<string>,
): void {
  const links = [
    ...markdown.matchAll(/\]\((\/sprawl-guard\/[^)\s]+)(?:\s+[^)]*)?\)/gu),
    ...markdown.matchAll(/^\[[^\]\n]+\]:\s+<?(\/sprawl-guard\/[^>\s]+)>?/gmu),
  ];
  for (const match of links) {
    const target = match[1];
    const pathname = target.split(/[?#]/u, 1)[0];
    if (pathname.endsWith(".md")) {
      const relativeTarget = pathname.slice(`${SITE_BASE_PATH}/`.length);
      assert.equal(
        expectedMarkdownPaths.has(relativeTarget),
        true,
        `${sourcePath} links to missing Markdown page ${target}`,
      );
    } else {
      assert.equal(
        expectedHtmlPaths.has(normalizeHtmlPath(pathname)),
        false,
        `${sourcePath} links to HTML documentation page ${target}`,
      );
    }
  }
}

function normalizeHtmlPath(path: string): string {
  return path.endsWith("/") ? path : `${path}/`;
}
