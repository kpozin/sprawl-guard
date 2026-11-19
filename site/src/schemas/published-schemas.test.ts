import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { describe, test } from "node:test";

import Ajv from "ajv-draft-04";
import { parse } from "smol-toml";

/** The site's public directory, which the documentation site serves at its root. */
const PUBLIC_DIRECTORY = new URL("../../public/", import.meta.url);

/** The documentation pages, whose titled TOML examples readers copy into their own files. */
const DOCS_DIRECTORY = new URL("../content/docs/", import.meta.url);

/** The configuration schema version that the documentation examples are written for. */
const CONFIG_SCHEMA_VERSION = 2;

/** The ratchet schema version that the invalid samples below are written for. */
const RATCHET_SCHEMA_VERSION = 1;

/** Matches a TOML code block and the file name that the documentation gives it as a title. */
const TITLED_TOML_EXAMPLE = /^[ \t]*```toml title="([^"]+)"\n([\s\S]*?)^[ \t]*```$/gm;

/**
 * Compiles a published schema with the Ajv options that SchemaStore checks schemas with. Strict
 * mode rejects unknown keywords and formats instead of ignoring them.
 */
function compile(path: string) {
  const schema: unknown = JSON.parse(readFileSync(new URL(path, PUBLIC_DIRECTORY), "utf8"));
  return new Ajv({
    allErrors: true,
    strict: true,
    strictTypes: false,
    strictTuples: false,
    allowMatchingProperties: true,
  }).compile(schema as object);
}

/** Lists the documentation's TOML examples titled `fileName`, each labeled with its page. */
function examples(fileName: string): { page: string; toml: string }[] {
  return readdirSync(DOCS_DIRECTORY, { recursive: true, encoding: "utf8" })
    .filter((page) => /\.mdx?$/.test(page))
    .flatMap((page) =>
      [...readFileSync(new URL(page, DOCS_DIRECTORY), "utf8").matchAll(TITLED_TOML_EXAMPLE)]
        .filter(([, title]) => title === fileName)
        .map(([, , toml]) => ({ page, toml: toml ?? "" })),
    );
}

describe("published schemas", () => {
  describe("the configuration schema", () => {
    const validate = compile("schemas/v2/sprawl-guard.schema.json");

    test("accepts every sprawl-guard.toml example in the documentation", () => {
      const configs = examples("sprawl-guard.toml");
      assert.ok(configs.length > 0, "the documentation has sprawl-guard.toml examples");
      for (const { page, toml } of configs) {
        // Most examples show one section, so they leave out the required schema version.
        const config = { schema_version: CONFIG_SCHEMA_VERSION, ...parse(toml) };
        assert.ok(validate(config), `${page}: ${JSON.stringify(validate.errors)}`);
      }
    });

    test("accepts the largest line limit the parser reads", () => {
      const config = {
        schema_version: CONFIG_SCHEMA_VERSION,
        limits: { max_lines_per_file: { production: 2 ** 32 - 1 } },
      };
      assert.ok(validate(config), JSON.stringify(validate.errors));
    });

    describe("when a byte limit is written", () => {
      /** Returns a configuration that sets every byte limit to `value`. */
      function withByteLimits(value: unknown, parsedCodeValue: unknown = value) {
        return {
          schema_version: CONFIG_SCHEMA_VERSION,
          limits: { max_bytes_per_file: value, max_bytes_per_lfs_object: value },
          traversal: { max_parsed_code_file_bytes: parsedCodeValue },
          overrides: [
            {
              paths: ["assets/**"],
              reason: "Assets",
              max_bytes_per_file: value,
              max_bytes_per_lfs_object: value,
            },
          ],
        };
      }

      for (const value of [500000, "512 B", "10 MB", "10MB", "1.5 GiB", "2 kB", "1 TiB"]) {
        test(`accepts ${JSON.stringify(value)}`, () => {
          assert.ok(validate(withByteLimits(value)), JSON.stringify(validate.errors));
        });
      }

      test('accepts "unlimited" for the file and LFS object limits', () => {
        assert.ok(validate(withByteLimits("unlimited", 4096)), JSON.stringify(validate.errors));
      });

      test('rejects "unlimited" for max_parsed_code_file_bytes', () => {
        assert.equal(validate(withByteLimits(4096, "unlimited")), false);
      });

      for (const value of [0, -1, 1.5, "10", "10M", "10 mb", "10 KB", "10  MB", "1e3 B", "1. MB"]) {
        test(`rejects ${JSON.stringify(value)}`, () => {
          assert.equal(validate(withByteLimits(value)), false);
        });
      }
    });

    describe("when a configuration is invalid", () => {
      for (const [name, config] of Object.entries({
        "has no schema version": { limits: {} },
        "has an unknown section": { schema_version: CONFIG_SCHEMA_VERSION, limit: {} },
        "sets a zero line limit": {
          schema_version: CONFIG_SCHEMA_VERSION,
          limits: { max_lines_per_file: { production: 0 } },
        },
        "sets a fractional line limit": {
          schema_version: CONFIG_SCHEMA_VERSION,
          limits: { max_lines_per_file: { production: 1.5 } },
        },
        "sets a line limit too large for the parser": {
          schema_version: CONFIG_SCHEMA_VERSION,
          limits: { max_lines_per_file: { production: 2 ** 32 } },
        },
        "has an override without a limit": {
          schema_version: CONFIG_SCHEMA_VERSION,
          overrides: [{ paths: ["src/**"], reason: "Generated code" }],
        },
      })) {
        test(`rejects one that ${name}`, () => {
          assert.equal(validate(config), false);
        });
      }
    });
  });

  describe("the ratchet schema", () => {
    const validate = compile("schemas/v1/sprawl-guard.ratchet.schema.json");

    test("accepts every sprawl-guard.ratchet.toml example in the documentation", () => {
      const ratchets = examples("sprawl-guard.ratchet.toml");
      assert.ok(ratchets.length > 0, "the documentation has sprawl-guard.ratchet.toml examples");
      for (const { page, toml } of ratchets) {
        assert.ok(validate(parse(toml)), `${page}: ${JSON.stringify(validate.errors)}`);
      }
    });

    describe("when a ratchet file is invalid", () => {
      for (const [name, ratchet] of Object.entries({
        "has no schema version": { files: [] },
        "has a file entry without a ceiling": {
          schema_version: RATCHET_SCHEMA_VERSION,
          files: [{ path: "src/lib.rs", role: "production" }],
        },
        "has a path outside the repository": {
          schema_version: RATCHET_SCHEMA_VERSION,
          files: [{ path: "../lib.rs", role: "production", max_lines: 2 }],
        },
      })) {
        test(`rejects one that ${name}`, () => {
          assert.equal(validate(ratchet), false);
        });
      }
    });
  });
});
