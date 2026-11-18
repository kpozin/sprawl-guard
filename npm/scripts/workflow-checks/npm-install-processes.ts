/**
 * Starts the processes that the npm install check runs: npm itself, and the installed wrapper's
 * entry point under Node.js. Uses only Node.js built-ins, like the check.
 */

import { type SpawnSyncOptionsWithStringEncoding, spawnSync } from "node:child_process";

/**
 * How long one command may run before it is stopped and counts as a failed attempt. npm waits up
 * to five minutes for each registry request, so a stalled connection could otherwise hold one
 * attempt far longer than the check's whole retry schedule.
 */
const COMMAND_TIMEOUT_MILLISECONDS = 120_000;

/** The outcome of one command whose stderr went straight to this process's stderr. */
export type CommandResult =
  | { readonly kind: "ok"; readonly stdout: string }
  | { readonly kind: "failed"; readonly detail: string };

/** The outcome of one command whose stderr was captured. */
export type CapturedCommandResult =
  | { readonly kind: "ok"; readonly stdout: string; readonly stderr: string }
  | { readonly kind: "failed"; readonly detail: string };

/** How to start a program with some arguments: as one shell command line, or directly. */
export type ProgramSpawn =
  | { readonly kind: "shell"; readonly commandLine: string }
  | { readonly kind: "direct"; readonly program: string; readonly args: readonly string[] };

/**
 * Returns how to start npm on a platform.
 *
 * On Windows npm is a `.cmd` script, which Node.js starts only through a shell. The name `npm`
 * stays unquoted: when cmd.exe finds a quoted batch name on `PATH`, the script sees the current
 * directory as its own and fails to find npm's JavaScript entry point. Each argument is quoted;
 * none can contain a double quote, which Windows paths, npm package names, versions, and
 * dist-tags never do.
 */
export function npmSpawn(platform: NodeJS.Platform, args: readonly string[]): ProgramSpawn {
  return platform === "win32"
    ? { kind: "shell", commandLine: ["npm", ...args.map((arg) => `"${arg}"`)].join(" ") }
    : { kind: "direct", program: "npm", args };
}

/** Runs npm and streams its stderr, so a slow install shows progress as it happens. */
export function runNpm(args: readonly string[]): CommandResult {
  return runProgram(npmSpawn(process.platform, args), "inherit");
}

/** Runs a JavaScript file with this Node.js and captures its stderr. */
export function runNodeScript(
  script: string,
  args: readonly string[],
): CapturedCommandResult {
  return runProgram({ kind: "direct", program: process.execPath, args: [script, ...args] }, "pipe");
}

/**
 * Runs a program with the wrapper's binary override removed, so the installed platform package
 * is the one that runs. Captured output is echoed, so the release log shows it. When stderr is
 * inherited, the result reports it as empty.
 */
function runProgram(spawn: ProgramSpawn, stderr: "inherit" | "pipe"): CapturedCommandResult {
  const env = { ...process.env };
  delete env.SPRAWL_GUARD_BINARY;
  const options: SpawnSyncOptionsWithStringEncoding = {
    encoding: "utf8",
    env,
    // npm can outlive SIGTERM, and `spawnSync` waits for the process to exit.
    killSignal: "SIGKILL",
    stdio: ["ignore", "pipe", stderr],
    timeout: COMMAND_TIMEOUT_MILLISECONDS,
  };
  const result =
    spawn.kind === "shell"
      ? spawnSync(spawn.commandLine, { ...options, shell: true })
      : spawnSync(spawn.program, spawn.args, options);
  process.stdout.write(result.stdout ?? "");
  process.stderr.write(result.stderr ?? "");
  if (result.error !== undefined) {
    return { kind: "failed", detail: result.error.message };
  }
  const commandLine =
    spawn.kind === "shell" ? spawn.commandLine : [spawn.program, ...spawn.args].join(" ");
  return result.status === 0
    ? { kind: "ok", stdout: result.stdout, stderr: result.stderr ?? "" }
    : { kind: "failed", detail: `${commandLine} exited with ${result.status ?? result.signal}` };
}
