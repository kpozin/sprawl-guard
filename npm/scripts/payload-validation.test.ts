import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import {
  absoluteDirectoryPath,
  type AbsoluteDirectoryPath,
  payloadFileRelativePath,
} from "./path-values.ts";
import {
  PAYLOAD_CHECK_KIND,
} from "./payload-check.ts";
import { validatePackagePayloads } from "./payload-validation.ts";

describe("validatePackagePayloads", () => {
  let tempRootDirectory: AbsoluteDirectoryPath;

  beforeEach(() => {
    tempRootDirectory = absoluteDirectoryPath(
      fs.mkdtempSync(path.join(os.tmpdir(), "sprawl-guard-payload-validation-")),
    );
  });

  afterEach(() => {
    fs.rmSync(tempRootDirectory, { recursive: true, force: true });
  });

  describe("when an expected path cannot be inspected", () => {
    it("reports the filesystem error instead of treating the payload as missing", () => {
      const packagePath = absoluteDirectoryPath(path.join(tempRootDirectory, "package"));
      fs.writeFileSync(packagePath, "not a directory\n");

      const result = validatePackagePayloads({
        packageDirectory: packagePath,
        payloadKind: PAYLOAD_CHECK_KIND.FILE,
        expectedPaths: [payloadFileRelativePath("payload")],
      });

      assert.equal(result.kind, "filesystem_error");
      if (result.kind === "filesystem_error") {
        assert.equal(result.relativePath, "payload");
        assert(result.cause instanceof Error);
        assert.notEqual((result.cause as NodeJS.ErrnoException).code, "ENOENT");
      }
    });
  });
});
