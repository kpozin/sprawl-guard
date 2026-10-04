import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { documentationRoutesFromPagePathnames } from "./markdown-output.ts";

describe("built documentation page discovery", () => {
  describe("when Astro omits drafts and includes its fallback page", () => {
    test("returns only emitted public documentation entries", () => {
      const result = documentationRoutesFromPagePathnames([
        "404.html",
        "",
        "quick-start/",
        "reference/configuration/",
      ]);

      assert.equal(result.ok, true);
      if (!result.ok) return;
      assert.deepEqual(
        result.value.entries.map((entry) => entry.entryId),
        ["index", "quick-start", "reference/configuration"],
      );
    });
  });

  describe("when Astro reports a page outside the documentation route grammar", () => {
    test("returns the offending pathname", () => {
      assert.deepEqual(documentationRoutesFromPagePathnames(["feed.xml"]), {
        ok: false,
        error: { kind: "invalid_page_pathname", pathname: "feed.xml" },
      });
    });
  });
});
