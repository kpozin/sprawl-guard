#!/usr/bin/env node

import { Result } from "better-result";
import { RuntimeFailureError } from "../lib/errors.js";
import { runCli } from "../lib/run-cli.js";

/** Exit code for failures in the wrapper itself, matching the CLI's operational-error code. */
const WRAPPER_FAILURE_EXIT_CODE = 2;

function formatUnexpectedError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

async function main(): Promise<void> {
  try {
    const result = await runCli(process.argv.slice(2));
    if (Result.isError(result)) {
      console.error(result.error.message);
      process.exit(WRAPPER_FAILURE_EXIT_CODE);
    }

    const outcome = result.value;
    if (outcome.kind === "signal") {
      const forwarded = Result.try({
        try: () => process.kill(process.pid, outcome.signal),
        catch: (cause) =>
          new RuntimeFailureError(
            "forward child signal",
            `failed to forward signal ${outcome.signal}`,
            cause,
          ),
      });
      if (Result.isError(forwarded)) {
        console.error(forwarded.error.message);
        process.exit(WRAPPER_FAILURE_EXIT_CODE);
      }
      process.exit(WRAPPER_FAILURE_EXIT_CODE);
    }

    process.exit(outcome.code);
  } catch (error) {
    console.error(formatUnexpectedError(error));
    process.exit(WRAPPER_FAILURE_EXIT_CODE);
  }
}

void main();
