import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import {
  loadPublicPackageVersion,
  PUBLIC_PACKAGE_VERSION_SCHEMA,
  PublicPackageVersionFileError,
} from "./package-version.ts";

describe("PUBLIC_PACKAGE_VERSION_SCHEMA", () => {
  describe("when a version cannot be represented consistently by Cargo and npm", () => {
    it("rejects unsafe numeric components, overlong values, and build metadata", () => {
      const overlongVersion = `1.0.0-${"a".repeat(251)}`;

      for (const version of [
        "9007199254740992.0.0",
        overlongVersion,
        "1.0.0+build.1",
      ]) {
        assert.equal(
          PUBLIC_PACKAGE_VERSION_SCHEMA.safeParse(version).success,
          false,
        );
      }
    });
  });
});

describe("loadPublicPackageVersion", () => {
  describe("when the version file contains one SemVer value", () => {
    it("returns the branded version", () => {
      const directory = fs.mkdtempSync(
        path.join(os.tmpdir(), "package-version-"),
      );
      try {
        const versionPath = path.join(directory, "package-version.json");
        fs.writeFileSync(versionPath, '{"version":"0.2.0-rc.1"}\n');

        assert.equal(loadPublicPackageVersion(versionPath), "0.2.0-rc.1");
      } finally {
        fs.rmSync(directory, { force: true, recursive: true });
      }
    });
  });

  describe("when the version file contains unexpected fields", () => {
    it("rejects the file", () => {
      const directory = fs.mkdtempSync(
        path.join(os.tmpdir(), "package-version-"),
      );
      try {
        const versionPath = path.join(directory, "package-version.json");
        fs.writeFileSync(
          versionPath,
          '{"version":"0.2.0","channel":"latest"}\n',
        );

        assert.throws(
          () => loadPublicPackageVersion(versionPath),
          (error) => {
            assert.ok(error instanceof PublicPackageVersionFileError);
            assert.equal(error.kind, "invalid_schema");
            assert.equal(error.versionPath, versionPath);
            return true;
          },
        );
      } finally {
        fs.rmSync(directory, { force: true, recursive: true });
      }
    });
  });

  describe("when the version file is missing", () => {
    it("throws a typed read failure with the source path", () => {
      const directory = fs.mkdtempSync(
        path.join(os.tmpdir(), "package-version-"),
      );
      const versionPath = path.join(directory, "missing-package-version.json");

      try {
        assert.throws(
          () => loadPublicPackageVersion(versionPath),
          (error) => {
            assert.ok(error instanceof PublicPackageVersionFileError);
            assert.equal(error.kind, "read_failed");
            assert.equal(error.versionPath, versionPath);
            return true;
          },
        );
      } finally {
        fs.rmSync(directory, { force: true, recursive: true });
      }
    });
  });

  describe("when the version file contains malformed JSON", () => {
    it("throws a typed parse failure", () => {
      const directory = fs.mkdtempSync(
        path.join(os.tmpdir(), "package-version-"),
      );
      try {
        const versionPath = path.join(directory, "package-version.json");
        fs.writeFileSync(versionPath, "{\n");

        assert.throws(
          () => loadPublicPackageVersion(versionPath),
          (error) => {
            assert.ok(error instanceof PublicPackageVersionFileError);
            assert.equal(error.kind, "invalid_json");
            return true;
          },
        );
      } finally {
        fs.rmSync(directory, { force: true, recursive: true });
      }
    });
  });
});
