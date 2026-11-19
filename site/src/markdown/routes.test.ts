import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { createDocumentationRoutes, rewriteDocumentationLink } from "./routes.ts";

describe("documentation routes", () => {
  describe("when the public collection contains root and nested entries", () => {
    test("maps each HTML page to one Markdown page", () => {
      const result = createDocumentationRoutes(["index", "quick-start", "reference/configuration"]);

      assert.equal(result.ok, true);
      if (!result.ok) return;

      assert.deepEqual(
        result.value.entries.map((entry) => ({
          htmlPath: entry.htmlPath,
          markdownPath: entry.markdownPath,
          routeSlug: entry.routeSlug,
        })),
        [
          {
            htmlPath: "/sprawl-guard/",
            markdownPath: "/sprawl-guard/index.md",
            routeSlug: "index",
          },
          {
            htmlPath: "/sprawl-guard/quick-start/",
            markdownPath: "/sprawl-guard/quick-start.md",
            routeSlug: "quick-start",
          },
          {
            htmlPath: "/sprawl-guard/reference/configuration/",
            markdownPath: "/sprawl-guard/reference/configuration.md",
            routeSlug: "reference/configuration",
          },
        ],
      );
    });
  });

  describe("when two IDs map to the same public path", () => {
    test("returns a route collision", () => {
      const result = createDocumentationRoutes(["index", "index"]);

      assert.deepEqual(result, {
        ok: false,
        error: {
          kind: "route_collision",
          path: "/sprawl-guard/",
        },
      });
    });
  });

  describe("when links target documentation and other resources", () => {
    test("rewrites only documentation targets and preserves suffixes", () => {
      const result = createDocumentationRoutes(["index", "quick-start", "reference/configuration"]);
      assert.equal(result.ok, true);
      if (!result.ok) return;
      const currentEntry = result.value.entries[1]?.entryId;
      assert.ok(currentEntry !== undefined);

      assert.equal(
        rewriteDocumentationLink(
          "/sprawl-guard/reference/configuration/?mode=full#limits",
          currentEntry,
          result.value,
        ),
        "/sprawl-guard/reference/configuration.md?mode=full#limits",
      );
      assert.equal(
        rewriteDocumentationLink(
          "/sprawl-guard/schemas/v1/config.json",
          currentEntry,
          result.value,
        ),
        "/sprawl-guard/schemas/v1/config.json",
      );
      assert.equal(
        rewriteDocumentationLink("https://example.com/docs/", currentEntry, result.value),
        "https://example.com/docs/",
      );
      assert.equal(rewriteDocumentationLink("#limits", currentEntry, result.value), "#limits");
    });
  });
});
