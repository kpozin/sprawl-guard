import { Result } from "better-result";
import { MissingFlagValueError, RuntimeFailureError, type WrapperError } from "./errors.js";
import {
  resolveAbsolutePath,
  unsafeHostRootPathFromAbsolutePath,
  type HostRootPath,
} from "./paths.js";

/**
 * Guest path where the host repository root is preopened.
 *
 * A WASI guest starts in `/`, so mounting the root there makes the CLI's working-directory
 * default select the repository without an injected `--root`.
 */
export const GUEST_ROOT = "/";

const ROOT_FLAG = "--root";
const ROOT_FLAG_PREFIX = `${ROOT_FLAG}=`;
const OPTION_TERMINATOR = "--";

/** Direct WASI invocation plus the host repository root preopened at {@link GUEST_ROOT}. */
export type WasiInvocationPlan = {
  readonly args: string[];
  readonly hostRoot: HostRootPath;
};

/** Position and value of the single `--root` option in the wrapper arguments. */
type RootOption = {
  readonly index: number;
  readonly tokenCount: 1 | 2;
  readonly value: string | undefined;
};

/**
 * Maps a host `--root` option into the WASI guest root.
 *
 * `--root` may appear anywhere before `--`; its value becomes the preopened host directory and
 * the option is rewritten in place to name {@link GUEST_ROOT}. Without `--root`, the current
 * directory is mounted. Every other token is passed to Rust unchanged, so non-root path operands
 * must be repository-relative when the WASI fallback is used.
 */
export function buildWasiInvocation(
  userArgv: readonly string[],
): Result<WasiInvocationPlan, WrapperError> {
  const rootOptions = findRootOptions(userArgv);
  const [rootOption, ...repeated] = rootOptions;
  if (repeated.length > 0) {
    return Result.err(
      new RuntimeFailureError(
        "prepare WASI invocation",
        `${ROOT_FLAG} may be passed only once`,
        userArgv,
      ),
    );
  }
  if (rootOption === undefined) {
    return Result.ok({
      args: ["sprawl-guard", ...userArgv],
      hostRoot: hostRootFrom(process.cwd()),
    });
  }
  if (rootOption.value === undefined || rootOption.value.length === 0) {
    return Result.err(new MissingFlagValueError(ROOT_FLAG));
  }

  const args = [...userArgv];
  args.splice(rootOption.index, rootOption.tokenCount, ROOT_FLAG, GUEST_ROOT);
  return Result.ok({
    args: ["sprawl-guard", ...args],
    hostRoot: hostRootFrom(rootOption.value),
  });
}

function hostRootFrom(path: string): HostRootPath {
  return unsafeHostRootPathFromAbsolutePath(resolveAbsolutePath(path));
}

function findRootOptions(argv: readonly string[]): RootOption[] {
  const options: RootOption[] = [];
  for (const [index, argument] of argv.entries()) {
    if (argument === OPTION_TERMINATOR) {
      break;
    }
    if (argument === ROOT_FLAG) {
      options.push({ index, tokenCount: 2, value: argv[index + 1] });
    } else if (argument.startsWith(ROOT_FLAG_PREFIX)) {
      options.push({ index, tokenCount: 1, value: argument.slice(ROOT_FLAG_PREFIX.length) });
    }
  }
  return options;
}
