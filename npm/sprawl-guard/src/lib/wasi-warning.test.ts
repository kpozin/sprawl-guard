import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { startWithoutWasiExperimentalWarning } from "./wasi-warning.js";

const WASI_EXPERIMENTAL_WARNING =
  "WASI is an experimental feature and might change at any time";

type WarningCall = readonly [warning: string | Error, ...args: unknown[]];

function warningHook(calls: WarningCall[]): typeof process.emitWarning {
  return ((warning: string | Error, ...args: unknown[]) => {
    calls.push([warning, ...args]);
  }) as typeof process.emitWarning;
}

describe("startWithoutWasiExperimentalWarning", () => {
  test("it suppresses only Node's WASI experimental warning", () => {
    const originalEmitWarning = process.emitWarning;
    const forwardedCalls: WarningCall[] = [];
    process.emitWarning = warningHook(forwardedCalls);

    try {
      startWithoutWasiExperimentalWarning(() => {
        process.emitWarning(WASI_EXPERIMENTAL_WARNING);
        process.emitWarning("an unrelated warning", { code: "OTHER_WARNING" });
      });

      assert.deepEqual(forwardedCalls, [
        ["an unrelated warning", { code: "OTHER_WARNING" }],
      ]);
    } finally {
      process.emitWarning = originalEmitWarning;
    }
  });

  test("it restores the previous warning hook after success or failure", () => {
    const originalEmitWarning = process.emitWarning;
    const previousHook = warningHook([]);
    process.emitWarning = previousHook;

    try {
      startWithoutWasiExperimentalWarning(() => undefined);
      assert.equal(process.emitWarning, previousHook);

      assert.throws(
        () =>
          startWithoutWasiExperimentalWarning(() => {
            throw new Error("loader failed");
          }),
        /loader failed/u,
      );
      assert.equal(process.emitWarning, previousHook);
    } finally {
      process.emitWarning = originalEmitWarning;
    }
  });

  test("it restores warning hooks before concurrent promises settle", async () => {
    const originalEmitWarning = process.emitWarning;
    const previousHook = warningHook([]);
    process.emitWarning = previousHook;

    try {
      const first = startWithoutWasiExperimentalWarning(async () => "first");
      assert.equal(process.emitWarning, previousHook);
      const second = startWithoutWasiExperimentalWarning(async () => "second");
      assert.equal(process.emitWarning, previousHook);

      assert.deepEqual(await Promise.all([first, second]), ["first", "second"]);
    } finally {
      process.emitWarning = originalEmitWarning;
    }
  });
});
