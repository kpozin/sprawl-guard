import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { Result } from "better-result";
import { MissingFlagValueError, RuntimeFailureError } from "./errors.js";
import { buildWasiInvocation } from "./wasi-invocation.js";

describe("buildWasiInvocation", () => {
  describe("when root is omitted", () => {
    test("it mounts the current directory and injects the guest root", () => {
      const result = buildWasiInvocation(["check", "src"]);

      assert.ok(Result.isOk(result));
      assert.deepEqual(result.value.args, [
        "sprawl-guard",
        "--root",
        "/workspace",
        "check",
        "src",
      ]);
    });
  });

  describe("when root is the leading argument", () => {
    test("it consumes the host path and passes repository-relative operands through", () => {
      const result = buildWasiInvocation([
        "--root",
        ".",
        "--config",
        "config/custom.toml",
        "check",
        "src",
      ]);

      assert.ok(Result.isOk(result));
      assert.deepEqual(result.value.args, [
        "sprawl-guard",
        "--root",
        "/workspace",
        "--config",
        "config/custom.toml",
        "check",
        "src",
      ]);
    });

    test("it accepts the equals form", () => {
      const result = buildWasiInvocation(["--root=.", "check"]);

      assert.ok(Result.isOk(result));
      assert.deepEqual(result.value.args, [
        "sprawl-guard",
        "--root",
        "/workspace",
        "check",
      ]);
    });
  });

  describe("when root has no value", () => {
    test("it returns the wrapper flag error", () => {
      const result = buildWasiInvocation(["--root"]);

      assert.ok(Result.isError(result));
      assert.ok(result.error instanceof MissingFlagValueError);
    });
  });

  describe("when root appears after another option", () => {
    test("it rejects the unsupported host-path placement", () => {
      const result = buildWasiInvocation(["--quiet", "--root", ".", "check"]);

      assert.ok(Result.isError(result));
      assert.ok(result.error instanceof RuntimeFailureError);
      assert.equal(result.error.operation, "prepare WASI invocation");
    });
  });

  describe("when a root-shaped operand appears after the option terminator", () => {
    test("it leaves the operand untouched", () => {
      const result = buildWasiInvocation(["check", "--", "--root"]);

      assert.ok(Result.isOk(result));
      assert.deepEqual(result.value.args, [
        "sprawl-guard",
        "--root",
        "/workspace",
        "check",
        "--",
        "--root",
      ]);
    });
  });
});
