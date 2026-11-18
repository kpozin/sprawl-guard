import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { loadWithoutWasiExperimentalWarning } from "./wasi-warning.js";

const WASI_EXPERIMENTAL_WARNING =
  "WASI is an experimental feature and might change at any time";

type WarningCall = readonly [warning: string | Error, ...args: unknown[]];

function warningHook(calls: WarningCall[]): typeof process.emitWarning {
  return ((warning: string | Error, ...args: unknown[]) => {
    calls.push([warning, ...args]);
  }) as typeof process.emitWarning;
}

/** Installs a recording warning hook for one test and puts Node's own hook back afterwards. */
async function withWarningHook(
  calls: WarningCall[],
  run: (hook: typeof process.emitWarning) => Promise<void>,
): Promise<void> {
  const originalEmitWarning = process.emitWarning;
  const hook = warningHook(calls);
  process.emitWarning = hook;
  try {
    await run(hook);
  } finally {
    process.emitWarning = originalEmitWarning;
  }
}

describe("loadWithoutWasiExperimentalWarning", () => {
  describe("when the load emits warnings after it first yields", () => {
    test("it suppresses only Node's WASI experimental warning", async () => {
      const forwardedCalls: WarningCall[] = [];
      await withWarningHook(forwardedCalls, async () => {
        await loadWithoutWasiExperimentalWarning(async () => {
          await Promise.resolve();
          process.emitWarning(WASI_EXPERIMENTAL_WARNING);
          process.emitWarning("an unrelated warning", { code: "OTHER_WARNING" });
        });

        assert.deepEqual(forwardedCalls, [
          ["an unrelated warning", { code: "OTHER_WARNING" }],
        ]);
      });
    });
  });

  describe("when the load settles", () => {
    test("it returns the loaded value and restores the previous hook", async () => {
      await withWarningHook([], async (previousHook) => {
        const loaded = await loadWithoutWasiExperimentalWarning(async () => "module");

        assert.equal(loaded, "module");
        assert.equal(process.emitWarning, previousHook);
      });
    });

    test("it rejects with the load's error and restores the previous hook", async () => {
      await withWarningHook([], async (previousHook) => {
        await assert.rejects(
          loadWithoutWasiExperimentalWarning(async () => {
            throw new Error("loader failed");
          }),
          /loader failed/u,
        );

        assert.equal(process.emitWarning, previousHook);
      });
    });
  });
});
