import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import type { ValidationResult } from "./npm-publication/model.ts";
import {
  NPM_ARCHIVE_PATH_SCHEMA,
  NPM_PACKAGE_INTEGRITY_SCHEMA,
} from "./npm-publication/values.ts";
import {
  validateArchiveIntegrity,
  validatePackedPaths,
  validatePnpmVersion,
  validateStagedPackageManifest,
} from "./npm-publication/validation.ts";

function invalidDetail(result: ValidationResult): string {
  assert.equal(result.kind, "invalid");
  return result.kind === "invalid" ? result.detail : "";
}

describe("validatePnpmVersion", () => {
  describe("when the active pnpm differs from the audited release client", () => {
    it("rejects the package manager before packing", () => {
      assert.match(
        invalidDetail(validatePnpmVersion("11.8.0")),
        /11\.28\.11 is required/u,
      );
    });
  });
});

describe("validateArchiveIntegrity", () => {
  describe("when a packed archive changes after candidate validation", () => {
    it("rejects the bytes before npm receives them", () => {
      const temporaryDirectory = fs.mkdtempSync(
        path.join(os.tmpdir(), "sprawl-guard-npm-integrity-test-"),
      );
      try {
        const archivePath = NPM_ARCHIVE_PATH_SCHEMA.parse(
          path.join(temporaryDirectory, "package.tgz"),
        );
        fs.writeFileSync(archivePath, "before");
        const expectedIntegrity = NPM_PACKAGE_INTEGRITY_SCHEMA.parse(
          "sha512-sq8t1sUVehdhDMueCwk/sEDkEamNpj+zyqDhv7lemkASgzGdIM3/4Uf4KW3fI12mLajqND+CM54AAUtj4V3NGg==",
        );
        fs.writeFileSync(archivePath, "after");

        assert.match(
          invalidDetail(validateArchiveIntegrity(archivePath, expectedIntegrity)),
          /archive integrity changed/u,
        );
      } finally {
        fs.rmSync(temporaryDirectory, { recursive: true });
      }
    });
  });
});

describe("staged npm package validation", () => {
  describe("when a staged manifest adds a lifecycle script", () => {
    it("rejects the manifest before pnpm runs", () => {
      assert.match(
        invalidDetail(
          validateStagedPackageManifest({
            actualText: '{"name":"package","scripts":{"prepack":"steal"}}\n',
            expectedText: '{"name":"package"}\n',
          }),
        ),
        /differs/u,
      );
    });
  });

  describe("when the archive adds an unexpected file", () => {
    it("rejects the exact packed path set", () => {
      assert.match(
        invalidDetail(
          validatePackedPaths({
            actualPaths: ["package.json", "README.md", "steal.js"],
            expectedPaths: ["package.json", "README.md"],
          }),
        ),
        /packed paths differ/u,
      );
    });
  });
});
