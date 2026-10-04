import { Result } from "better-result";
import { MissingFlagValueError, RuntimeFailureError, type WrapperError } from "./errors.js";
import {
  resolveAbsolutePath,
  unsafeHostRootPathFromAbsolutePath,
  type HostRootPath,
} from "./paths.js";

const GUEST_WORKSPACE_ROOT = "/workspace";

/** Direct WASI invocation plus the host repository root preopened as `/workspace`. */
export type WasiInvocationPlan = {
  readonly args: string[];
  readonly hostRoot: HostRootPath;
};

/**
 * Maps the wrapper-owned leading host `--root` into the WASI guest workspace.
 *
 * Every other token is passed to Rust unchanged. Non-root path operands must therefore be
 * repository-relative when the WASI fallback is used.
 */
export function buildWasiInvocation(
  userArgv: readonly string[],
): Result<WasiInvocationPlan, WrapperError> {
  let hostRoot = unsafeHostRootPathFromAbsolutePath(resolveAbsolutePath(process.cwd()));
  let remaining = [...userArgv];

  const first = remaining[0];
  if (first === "--root") {
    const rawRoot = remaining[1];
    if (rawRoot === undefined || rawRoot.length === 0) {
      return Result.err(new MissingFlagValueError("--root"));
    }
    hostRoot = unsafeHostRootPathFromAbsolutePath(resolveAbsolutePath(rawRoot));
    remaining = remaining.slice(2);
  } else if (first?.startsWith("--root=") === true) {
    const rawRoot = first.slice("--root=".length);
    if (rawRoot.length === 0) {
      return Result.err(new MissingFlagValueError("--root"));
    }
    hostRoot = unsafeHostRootPathFromAbsolutePath(resolveAbsolutePath(rawRoot));
    remaining = remaining.slice(1);
  }

  if (containsRootOption(remaining)) {
    return Result.err(
      new RuntimeFailureError(
        "prepare WASI invocation",
        "the WASI fallback requires --root to be the first wrapper argument",
        userArgv,
      ),
    );
  }

  return Result.ok({
    args: ["sprawl-guard", "--root", GUEST_WORKSPACE_ROOT, ...remaining],
    hostRoot,
  });
}

function containsRootOption(argv: readonly string[]): boolean {
  for (const argument of argv) {
    if (argument === "--") {
      return false;
    }
    if (argument === "--root" || argument.startsWith("--root=")) {
      return true;
    }
  }
  return false;
}
