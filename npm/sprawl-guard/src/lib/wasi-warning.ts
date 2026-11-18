const WASI_EXPERIMENTAL_WARNING =
  "WASI is an experimental feature and might change at any time";

/**
 * Loads a module without exposing Node's WASI experimental warning. Node 22 emits the warning
 * while it evaluates `node:wasi`, which happens after a dynamic `import()` returns its promise, so
 * the filter stays in place until the load settles. Calls must not overlap, because each one
 * restores the warning hook it found.
 */
export async function loadWithoutWasiExperimentalWarning<T>(load: () => Promise<T>): Promise<T> {
  const originalEmitWarning = process.emitWarning;
  const filteredEmitWarning = ((warning: string | Error, ...args: unknown[]) => {
    const message = warning instanceof Error ? warning.message : warning;
    if (message === WASI_EXPERIMENTAL_WARNING) {
      return;
    }
    Reflect.apply(originalEmitWarning, process, [warning, ...args]);
  }) as typeof process.emitWarning;
  process.emitWarning = filteredEmitWarning;

  try {
    return await load();
  } finally {
    process.emitWarning = originalEmitWarning;
  }
}
