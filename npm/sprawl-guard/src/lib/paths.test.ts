import { describe, test } from "node:test";
import { unsafeAbsolutePathFromKnownAbsolute } from "./paths.js";
import type { AbsolutePath } from "./paths.js";

const literalAbsolutePath = unsafeAbsolutePathFromKnownAbsolute("/repo/package.json" as const);

const absoluteLiteralTypeCheck: "/repo/package.json" & AbsolutePath = literalAbsolutePath;

void absoluteLiteralTypeCheck;

describe("unsafeAbsolutePathFromKnownAbsolute", () => {
  describe("when the input is a string literal", () => {
    test("preserves the literal type while applying the brand", () => {
      const branded = unsafeAbsolutePathFromKnownAbsolute("/repo/package.json" as const);
      const typeCheck: "/repo/package.json" & AbsolutePath = branded;
      void typeCheck;
    });
  });
});
