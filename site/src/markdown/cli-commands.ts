import { z } from "astro/zod";
import type { Code } from "mdast";

import type { Result } from "./routes.ts";
import { syncKeySchema, tabLabelSchema, type SyncKey, type TabLabel } from "./synced-tabs.ts";

/**
 * The first word of a code fence's meta that marks the block as Sprawl Guard commands, as in
 * ` ```sh sprawl-guard title="Run the first check" `. The HTML site shows such a block as one tab
 * per installation; the Markdown output keeps it as written.
 */
export const CLI_COMMANDS_FLAG = "sprawl-guard";

/** The program name that every line of a command block starts with. */
export const PROGRAM_NAME = "sprawl-guard";

/** The only fence language a command block may use. */
const SHELL_LANGUAGE = "sh";

/** The Starlight tab sync key, shared with other package-manager tabs on the site. */
export const PACKAGE_MANAGER_SYNC_KEY: SyncKey = syncKeySchema.parse("package-manager");

/**
 * What a reader types to start Sprawl Guard as one installation provides it, such as
 * `pnpm exec sprawl-guard`, with no arguments.
 */
const programInvocationSchema = z
  .string()
  .regex(/^\S(?:.*\S)?$/su)
  .brand<"ProgramInvocation">();
export type ProgramInvocation = z.infer<typeof programInvocationSchema>;

/** One way to install Sprawl Guard. */
export interface Installation {
  /** The label of the installation's tab. */
  readonly label: TabLabel;
  /** How this installation starts Sprawl Guard. */
  readonly command: ProgramInvocation;
}

/** The ways a reader can install Sprawl Guard, in tab order. */
export const INSTALLATIONS: readonly Installation[] = (
  [
    ["pnpm", "pnpm exec sprawl-guard"],
    ["npm", "npm exec -- @kpozin/sprawl-guard"],
    ["uv", "uv run sprawl-guard"],
    ["Cargo", "sprawl-guard"],
    ["Homebrew", "sprawl-guard"],
  ] as const
).map(([label, command]) => ({
  label: tabLabelSchema.parse(label),
  command: programInvocationSchema.parse(command),
}));

/**
 * A command as the documentation writes it, kept as its text after the program name, so that it
 * holds for every installation: empty, or the arguments with their leading space, such as ` check`
 * in `sprawl-guard check`.
 */
const commandArgumentsSchema = z
  .string()
  .regex(/^(?: .*)?$/su)
  .brand<"CommandArguments">();
export type CommandArguments = z.infer<typeof commandArgumentsSchema>;

/** A command that one installation runs as written, such as `pnpm exec sprawl-guard check`. */
const installedCommandSchema = z.string().min(1).brand<"InstalledCommand">();
export type InstalledCommand = z.infer<typeof installedCommandSchema>;

/** Writes a documented command the way that `installation` runs it. */
export function commandFor(
  installation: Installation,
  argumentsText: CommandArguments,
): InstalledCommand {
  return installedCommandSchema.parse(`${installation.command}${argumentsText}`);
}

/**
 * A code block of Sprawl Guard commands. Each line is kept as the text after the program name, so
 * a variant for any installation is that installation's command followed by the line.
 */
export interface CliCommandBlock {
  /** The fence meta after the flag, such as a `title`, or `null` when there is none, as in mdast. */
  readonly meta: string | null;
  /** Each command's text after the program name. */
  readonly argumentLines: readonly CommandArguments[];
}

/** What a code block is, as far as command tabs are concerned. */
export type CodeBlockKind =
  /** A block flagged as Sprawl Guard commands. */
  | { readonly kind: "cli_commands"; readonly block: CliCommandBlock }
  /**
   * An unflagged shell block whose every line runs Sprawl Guard, which usually means the author
   * forgot the flag.
   */
  | { readonly kind: "unflagged_cli_commands" }
  /** Any other code block. */
  | { readonly kind: "other" };

/** Why a flagged code block can't be shown as command tabs. */
export type CliCommandBlockError =
  | { readonly kind: "not_a_shell_block"; readonly lang: string | null | undefined }
  | { readonly kind: "not_a_command"; readonly line: string };

/** Classifies a code block, rejecting a flagged block that isn't a list of Sprawl Guard commands. */
export function parseCodeBlock(
  node: Pick<Code, "lang" | "meta" | "value">,
): Result<CodeBlockKind, CliCommandBlockError> {
  const meta = node.meta?.trim() ?? "";
  const flagEnd = meta.search(/\s|$/u);
  const lines = node.value.split("\n");
  if (meta.slice(0, flagEnd) !== CLI_COMMANDS_FLAG) {
    const unflagged =
      node.lang === SHELL_LANGUAGE && lines.every((line) => commandArguments(line) !== undefined);
    return { ok: true, value: { kind: unflagged ? "unflagged_cli_commands" : "other" } };
  }
  if (node.lang !== SHELL_LANGUAGE) {
    return { ok: false, error: { kind: "not_a_shell_block", lang: node.lang } };
  }

  const argumentLines: CommandArguments[] = [];
  for (const line of lines) {
    const argumentsText = commandArguments(line);
    if (argumentsText === undefined) return { ok: false, error: { kind: "not_a_command", line } };
    argumentLines.push(argumentsText);
  }
  const rest = meta.slice(flagEnd).trim();
  return {
    ok: true,
    value: {
      kind: "cli_commands",
      block: { meta: rest === "" ? null : rest, argumentLines },
    },
  };
}

/** Writes a command block's commands for one installation. */
export function commandsFor(block: CliCommandBlock, installation: Installation): string {
  return block.argumentLines.map((line) => commandFor(installation, line)).join("\n");
}

/**
 * Returns the arguments of a command written as inline code, such as ` check` in
 * `sprawl-guard check`, or `undefined` for other code, including the bare program name, which
 * names the program rather than running it.
 */
export function inlineCommandArguments(value: string): CommandArguments | undefined {
  const rest = commandArguments(value);
  return rest === "" ? undefined : rest;
}

/** Describes a command block error for a build failure. */
export function formatCliCommandBlockError(error: CliCommandBlockError): string {
  switch (error.kind) {
    case "not_a_shell_block":
      return `a ${CLI_COMMANDS_FLAG} code block must be \`${SHELL_LANGUAGE}\`, not ${JSON.stringify(error.lang)}`;
    case "not_a_command":
      return `every line of a ${CLI_COMMANDS_FLAG} code block must run ${PROGRAM_NAME}: ${JSON.stringify(error.line)}`;
  }
}

/** Returns a line's text after the program name, or `undefined` when the line doesn't run it. */
function commandArguments(line: string): CommandArguments | undefined {
  if (!line.startsWith(PROGRAM_NAME)) return undefined;
  const parsed = commandArgumentsSchema.safeParse(line.slice(PROGRAM_NAME.length));
  return parsed.success ? parsed.data : undefined;
}
