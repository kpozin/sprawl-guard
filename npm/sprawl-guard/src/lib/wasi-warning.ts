const WASI_EXPERIMENTAL_WARNING =
  "WASI is an experimental feature and might change at any time";

/** Starts an operation without exposing Node's WASI experimental warning. */
export function startWithoutWasiExperimentalWarning<T>(operation: () => T): T {
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
    return operation();
  } finally {
    process.emitWarning = originalEmitWarning;
  }
}
