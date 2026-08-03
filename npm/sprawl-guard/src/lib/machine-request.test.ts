import assert from "node:assert/strict";
import path from "node:path";
import { describe, test } from "node:test";
import { Result } from "better-result";
import { MissingFlagValueError } from "./errors.js";
import {
  MACHINE_PROTOCOL_VERSION,
  MachineHostPathStyle,
} from "./machine-contract.js";
import { buildMachineRunRequest } from "./machine-request.js";

const expectedHostPathStyle =
  process.platform === "win32"
    ? MachineHostPathStyle.Windows
    : MachineHostPathStyle.Posix;

describe("buildMachineRunRequest", () => {
  describe("when argv includes a separate root value", () => {
    test("it uses the host root for preopen setup and rewrites only the root value", () => {
      const request = buildMachineRunRequest([
        "--root",
        "/repo",
        "--config",
        "/repo/config/sprawl-guard.toml",
        "check",
        "--format",
        "json",
        "/repo/src/lib.rs",
      ]);

      assert.ok(Result.isOk(request));
      assert.equal(request.value.hostRoot, path.resolve("/repo"));
      assert.equal(request.value.hostPathStyle, expectedHostPathStyle);
      assert.deepEqual(JSON.parse(request.value.requestJson), {
        protocol_version: MACHINE_PROTOCOL_VERSION,
        workspace_root: "/workspace",
        host_path_mapping: {
          kind: "alias",
          host_root: path.resolve("/repo"),
          style: expectedHostPathStyle,
        },
        argv: [
          "--root",
          "/workspace",
          "--config",
          "/repo/config/sprawl-guard.toml",
          "check",
          "--format",
          "json",
          "/repo/src/lib.rs",
        ],
      });
    });
  });

  describe("when argv includes an inline root value", () => {
    test("it rewrites the inline root to the guest workspace root", () => {
      const request = buildMachineRunRequest([
        "--root=/repo",
        "explain",
        "/repo/src/lib.rs",
      ]);

      assert.ok(Result.isOk(request));
      assert.deepEqual(JSON.parse(request.value.requestJson), {
        protocol_version: MACHINE_PROTOCOL_VERSION,
        workspace_root: "/workspace",
        host_path_mapping: {
          kind: "alias",
          host_root: path.resolve("/repo"),
          style: expectedHostPathStyle,
        },
        argv: ["--root=/workspace", "explain", "/repo/src/lib.rs"],
      });
    });
  });

  describe("when argv does not include a root", () => {
    test("it uses the current working directory and preserves every token", () => {
      const request = buildMachineRunRequest(["baseline", "--write"]);

      assert.ok(Result.isOk(request));
      assert.equal(request.value.hostRoot, path.resolve(process.cwd()));
      assert.deepEqual(JSON.parse(request.value.requestJson), {
        protocol_version: MACHINE_PROTOCOL_VERSION,
        workspace_root: "/workspace",
        host_path_mapping: {
          kind: "alias",
          host_root: path.resolve(process.cwd()),
          style: expectedHostPathStyle,
        },
        argv: ["baseline", "--write"],
      });
    });
  });

  describe("when argv includes an argument terminator", () => {
    test("it stops root detection and preserves later tokens", () => {
      const request = buildMachineRunRequest(["--", "--root", "/other"]);

      assert.ok(Result.isOk(request));
      assert.deepEqual(JSON.parse(request.value.requestJson).argv, [
        "--",
        "--root",
        "/other",
      ]);
      assert.equal(request.value.hostRoot, path.resolve(process.cwd()));
    });
  });

  describe("when argv includes duplicate root flags", () => {
    test("it preserves the duplicate flags while using the last root for the preopen", () => {
      const request = buildMachineRunRequest([
        "--root",
        "/repo",
        "--root=/repo/packages/api",
        "check",
      ]);

      assert.ok(Result.isOk(request));
      assert.equal(request.value.hostRoot, path.resolve("/repo/packages/api"));
      assert.deepEqual(JSON.parse(request.value.requestJson).argv, [
        "--root",
        "/workspace",
        "--root=/workspace",
        "check",
      ]);
    });
  });

  describe("when a root flag is missing its value", () => {
    test("it returns a typed setup error before launching WASI", () => {
      const request = buildMachineRunRequest(["--root"]);

      assert.ok(Result.isError(request));
      assert.ok(request.error instanceof MissingFlagValueError);
      assert.equal(request.error.flag, "--root");
    });
  });

  describe("when a separate root value is empty", () => {
    test("it returns a typed setup error before launching WASI", () => {
      const request = buildMachineRunRequest(["--root", ""]);

      assert.ok(Result.isError(request));
      assert.ok(request.error instanceof MissingFlagValueError);
      assert.equal(request.error.flag, "--root");
    });
  });

  describe("when a value-taking global option appears before a root-like token", () => {
    test("it preserves the token as the option value instead of using it for preopen", () => {
      const request = buildMachineRunRequest(["--config", "--root", "check"]);

      assert.ok(Result.isOk(request));
      assert.equal(request.value.hostRoot, path.resolve(process.cwd()));
      assert.deepEqual(JSON.parse(request.value.requestJson).argv, [
        "--config",
        "--root",
        "check",
      ]);
    });
  });

  describe("when an unknown top-level flag appears before a root flag", () => {
    test("it stops root detection so Rust can report the unknown flag", () => {
      const request = buildMachineRunRequest([
        "--bogus",
        "--root",
        "/repo",
        "check",
      ]);

      assert.ok(Result.isOk(request));
      assert.equal(request.value.hostRoot, path.resolve(process.cwd()));
      assert.deepEqual(JSON.parse(request.value.requestJson).argv, [
        "--bogus",
        "--root",
        "/repo",
        "check",
      ]);
    });
  });

  describe("when a removed init language flag appears before a root flag", () => {
    test("it leaves root handling to Rust instead of consuming obsolete option grammar", () => {
      for (const argv of [
        ["init", "--languages", "rust", "--root", "/repo"],
        ["init", "--all-languages", "--root", "/repo"],
      ]) {
        const request = buildMachineRunRequest(argv);

        assert.ok(Result.isOk(request));
        assert.equal(request.value.hostRoot, path.resolve(process.cwd()));
        assert.deepEqual(JSON.parse(request.value.requestJson).argv, argv);
      }
    });
  });

  describe("when root appears after the command", () => {
    test("it uses the root for preopen selection and rewrites the value", () => {
      const request = buildMachineRunRequest(["check", "--root", "/repo"]);

      assert.ok(Result.isOk(request));
      assert.equal(request.value.hostRoot, path.resolve("/repo"));
      assert.deepEqual(JSON.parse(request.value.requestJson).argv, [
        "check",
        "--root",
        "/workspace",
      ]);
    });
  });

  describe("when inline root appears after the command", () => {
    test("it uses the root for preopen selection and rewrites the value", () => {
      const request = buildMachineRunRequest(["check", "--root=/repo"]);

      assert.ok(Result.isOk(request));
      assert.equal(request.value.hostRoot, path.resolve("/repo"));
      assert.deepEqual(JSON.parse(request.value.requestJson).argv, [
        "check",
        "--root=/workspace",
      ]);
    });
  });

  describe("when a command option consumes a root-like token", () => {
    test("it preserves the token as the option value instead of using it for preopen", () => {
      const request = buildMachineRunRequest(["check", "--format", "--root"]);

      assert.ok(Result.isOk(request));
      assert.equal(request.value.hostRoot, path.resolve(process.cwd()));
      assert.deepEqual(JSON.parse(request.value.requestJson).argv, [
        "check",
        "--format",
        "--root",
      ]);
    });
  });
});
