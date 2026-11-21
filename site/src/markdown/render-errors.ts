import { formatCliCommandBlockError, type CliCommandBlockError } from "./cli-commands.ts";
import type { CliReferenceError } from "./cli-reference.ts";
import type { DocsEntryId, DocumentationRouteError, Result } from "./routes.ts";

/** A reason a documentation entry can't be rendered as Markdown. */
export type MarkdownRenderError =
  | {
      readonly kind: "invalid_entry_id";
      readonly entryId: string;
    }
  | {
      readonly kind: "missing_documentation_route";
      readonly entryId: DocsEntryId;
    }
  | {
      readonly kind: "unsupported_mdx_construct";
      readonly entryId: DocsEntryId;
      readonly construct: string;
    }
  | {
      readonly kind: "malformed_mdx_component";
      readonly entryId: DocsEntryId;
      readonly component: string;
      readonly detail: string;
    }
  | {
      readonly kind: "cli_reference";
      readonly entryId: DocsEntryId;
      readonly cause: CliReferenceError;
    }
  | {
      readonly kind: "cli_command_block";
      readonly entryId: DocsEntryId;
      readonly cause: CliCommandBlockError;
    }
  | {
      /** A shell block that runs only Sprawl Guard but lacks the flag that gives it tabs. */
      readonly kind: "unflagged_cli_command_block";
      readonly entryId: DocsEntryId;
      readonly value: string;
    };

/** Formats a structured rendering failure at an Astro build boundary. */
export function formatMarkdownRenderError(error: MarkdownRenderError): string {
  switch (error.kind) {
    case "invalid_entry_id":
      return `Invalid documentation entry ID: ${JSON.stringify(error.entryId)}`;
    case "missing_documentation_route":
      return `Missing Markdown route for documentation entry: ${error.entryId}`;
    case "unsupported_mdx_construct":
      return `Unsupported MDX construct in ${error.entryId}: ${error.construct}`;
    case "malformed_mdx_component":
      return `Malformed ${error.component} in ${error.entryId}: ${error.detail}`;
    case "cli_reference":
      return `Invalid CLI reference in ${error.entryId}: ${error.cause.kind}`;
    case "cli_command_block":
      return `Invalid command block in ${error.entryId}: ${formatCliCommandBlockError(error.cause)}`;
    case "unflagged_cli_command_block":
      return `Command block in ${error.entryId} needs the sprawl-guard fence flag: ${JSON.stringify(error.value)}`;
  }
}

/** Turns the route error from parsing an entry ID into a rendering failure. */
export function invalidEntryId(error: DocumentationRouteError): Result<never, MarkdownRenderError> {
  if (error.kind === "invalid_entry_id") {
    return { ok: false, error: { kind: "invalid_entry_id", entryId: error.entryId } };
  }
  throw new Error(`Unexpected route error while parsing an entry ID: ${error.kind}`);
}

/** Reports a construct that has no Markdown form. */
export function unsupported(
  entryId: DocsEntryId,
  construct: string,
): Result<never, MarkdownRenderError> {
  return {
    ok: false,
    error: { kind: "unsupported_mdx_construct", entryId, construct },
  };
}

/** Reports a supported component with the wrong attributes or children. */
export function malformed(
  entryId: DocsEntryId,
  component: string,
  detail: string,
): Result<never, MarkdownRenderError> {
  return {
    ok: false,
    error: { kind: "malformed_mdx_component", entryId, component, detail },
  };
}
