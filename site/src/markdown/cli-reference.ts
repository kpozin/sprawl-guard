import { z } from "astro/zod";

import cliReferenceJson from "../generated/cli-reference.json" with { type: "json" };

const cliReferencePossibleValueSchema = z
  .object({
    name: z.string(),
    help: z.string().nullable(),
  })
  .strict();

const cliReferenceArgumentSchema = z
  .object({
    id: z.string(),
    syntax: z.string(),
    help: z.string().nullable(),
    conflicts_with: z.array(z.string()),
    default_values: z.array(z.string()),
    possible_values: z.array(cliReferencePossibleValueSchema),
  })
  .strict();

const cliReferenceCommandSchema = z
  .object({
    path: z.array(z.string()).min(1),
    usage: z.string(),
    arguments: z.array(cliReferenceArgumentSchema),
  })
  .strict();

const cliReferenceDocumentSchema = z
  .object({
    schema_version: z.literal(2),
    commands: z.array(cliReferenceCommandSchema),
  })
  .strict();

const cliCommandPathSchema = z.string().trim().min(1).brand<"CliCommandPath">();

export type CliReferenceCommand = z.infer<typeof cliReferenceCommandSchema>;
export type CliCommandPath = z.infer<typeof cliCommandPathSchema>;

export type CliReferenceError =
  | { readonly kind: "invalid_cli_reference"; readonly detail: string }
  | { readonly kind: "invalid_cli_command_path"; readonly path: string }
  | { readonly kind: "unknown_cli_command_path"; readonly path: CliCommandPath }
  | { readonly kind: "ambiguous_cli_command_path"; readonly path: CliCommandPath };

export type InlineSegment =
  | { readonly kind: "text"; readonly value: string }
  | { readonly kind: "code"; readonly value: string };

type Result<T, E> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };

const parsedCliReference = cliReferenceDocumentSchema.safeParse(cliReferenceJson);

/** Selects one generated CLI command by its full command path. */
export function findCliReferenceCommand(
  rawPath: string,
): Result<CliReferenceCommand, CliReferenceError> {
  if (!parsedCliReference.success) {
    return {
      ok: false,
      error: {
        kind: "invalid_cli_reference",
        detail: z.prettifyError(parsedCliReference.error),
      },
    };
  }

  const parsedPath = cliCommandPathSchema.safeParse(rawPath);
  if (!parsedPath.success) {
    return {
      ok: false,
      error: { kind: "invalid_cli_command_path", path: rawPath },
    };
  }

  const [match, ...others] = parsedCliReference.data.commands.filter(
    (command) => command.path.join(" ") === parsedPath.data,
  );
  if (match === undefined) {
    return {
      ok: false,
      error: { kind: "unknown_cli_command_path", path: parsedPath.data },
    };
  }
  if (others.length > 0) {
    return {
      ok: false,
      error: { kind: "ambiguous_cli_command_path", path: parsedPath.data },
    };
  }
  return { ok: true, value: match };
}

/** Splits backtick-delimited inline code from generated help text. */
export function inlineCodeSegments(value: string): InlineSegment[] {
  const segments: InlineSegment[] = [];
  const inlineCode = /`([^`]+)`/gu;
  let plainTextStart = 0;

  for (const match of value.matchAll(inlineCode)) {
    const matchStart = match.index;
    if (plainTextStart < matchStart) {
      segments.push({ kind: "text", value: value.slice(plainTextStart, matchStart) });
    }
    segments.push({ kind: "code", value: match[1] ?? "" });
    plainTextStart = matchStart + match[0].length;
  }

  if (plainTextStart < value.length) {
    segments.push({ kind: "text", value: value.slice(plainTextStart) });
  }
  return segments;
}

/** Formats a structured CLI reference failure at an Astro build boundary. */
export function formatCliReferenceError(error: CliReferenceError): string {
  switch (error.kind) {
    case "invalid_cli_reference":
      return `Invalid Sprawl Guard CLI reference: ${error.detail}`;
    case "invalid_cli_command_path":
      return `Invalid Sprawl Guard CLI command path: ${JSON.stringify(error.path)}`;
    case "unknown_cli_command_path":
      return `Unknown Sprawl Guard CLI command path: ${error.path}`;
    case "ambiguous_cli_command_path":
      return `Ambiguous Sprawl Guard CLI command path: ${error.path}`;
  }
}
