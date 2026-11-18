import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { npmSpawn } from "./npm-install-processes.ts";

describe("npmSpawn", () => {
  describe("on Windows", () => {
    it("starts npm through the shell by its bare name, with each argument quoted", () => {
      assert.deepEqual(npmSpawn("win32", ["install", "C:\\Temp Dir\\wrapper.tgz"]), {
        kind: "shell",
        commandLine: 'npm "install" "C:\\Temp Dir\\wrapper.tgz"',
      });
    });
  });

  describe("on other platforms", () => {
    it("starts npm directly with its arguments", () => {
      assert.deepEqual(npmSpawn("linux", ["install", "/tmp/dir with space/wrapper.tgz"]), {
        kind: "direct",
        program: "npm",
        args: ["install", "/tmp/dir with space/wrapper.tgz"],
      });
    });
  });
});
