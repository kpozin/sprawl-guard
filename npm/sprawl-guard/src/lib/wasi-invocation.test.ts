import assert from "node:assert/strict";
import path from "node:path";
import { describe, test } from "node:test";
import { Result } from "better-result";
import { MissingFlagValueError, RuntimeFailureError } from "./errors.js";
import { buildWasiInvocation } from "./wasi-invocation.js";

describe("buildWasiInvocation", () => {
  describe("when root is omitted", () => {
    test("it mounts the current directory and passes the arguments unchanged", () => {
      const result = buildWasiInvocation(["check", "src"]);

      assert.ok(Result.isOk(result));
      assert.deepEqual(result.value.args, ["sprawl-guard", "check", "src"]);
      assert.equal(result.value.hostRoot, path.resolve(process.cwd()));
    });
  });

  describe("when root follows the subcommand", () => {
    test("it mounts the host path and rewrites the option in place", () => {
      const result = buildWasiInvocation([
        "check",
        "--root",
        "fixture",
        "--config",
        "config/custom.toml",
        "src",
      ]);

      assert.ok(Result.isOk(result));
      assert.deepEqual(result.value.args, [
        "sprawl-guard",
        "check",
        "--root",
        "/",
        "--config",
        "config/custom.toml",
        "src",
      ]);
      assert.equal(result.value.hostRoot, path.resolve("fixture"));
    });

    test("it accepts the equals form", () => {
      const result = buildWasiInvocation(["config", "resolved", "--root=fixture"]);

      assert.ok(Result.isOk(result));
      assert.deepEqual(result.value.args, ["sprawl-guard", "config", "resolved", "--root", "/"]);
      assert.equal(result.value.hostRoot, path.resolve("fixture"));
    });
  });

  describe("when root has no value", () => {
    for (const args of [["check", "--root"], ["check", "--root="]]) {
      test(`it returns the wrapper flag error for ${args.join(" ")}`, () => {
        const result = buildWasiInvocation(args);

        assert.ok(Result.isError(result));
        assert.ok(result.error instanceof MissingFlagValueError);
      });
    }
  });

  describe("when root is repeated", () => {
    test("it rejects the ambiguous host path", () => {
      const result = buildWasiInvocation(["check", "--root", "a", "--root=b"]);

      assert.ok(Result.isError(result));
      assert.ok(result.error instanceof RuntimeFailureError);
      assert.equal(result.error.operation, "prepare WASI invocation");
    });
  });

  describe("when a root-shaped operand appears after the option terminator", () => {
    test("it leaves the operand untouched", () => {
      const result = buildWasiInvocation(["check", "--", "--root"]);

      assert.ok(Result.isOk(result));
      assert.deepEqual(result.value.args, ["sprawl-guard", "check", "--", "--root"]);
    });
  });
});
