import type { Paragraph, PhrasingContent } from "mdast";
import type { MdastNode } from "satteri";

import { INSTALLATIONS, type Installation, parseCodeBlock, PROGRAM_NAME } from "./cli-commands.ts";

/** Joins alternatives as English prose does, as in "pnpm, npm, or uv". */
const ALTERNATIVES = new Intl.ListFormat("en", { type: "disjunction" });

/** Reports whether any code block in `nodes`, however deeply nested, is flagged as commands. */
export function containsCliCommandBlock(nodes: readonly MdastNode[]): boolean {
  return nodes.some((node) => {
    if (node.type === "code") {
      const parsed = parseCodeBlock(node);
      return parsed.ok && parsed.value.kind === "cli_commands";
    }
    return (
      "children" in node && Array.isArray(node.children) && containsCliCommandBlock(node.children)
    );
  });
}

/**
 * Tells readers of the agent-readable Markdown how to run a page's commands when they installed
 * Sprawl Guard through a package manager, since that output has no tabs.
 */
export function installationNote(): Paragraph {
  const direct = INSTALLATIONS.filter((installation) => installation.command === PROGRAM_NAME);
  const packageManagers = INSTALLATIONS.filter(
    (installation) => installation.command !== PROGRAM_NAME,
  );
  const labelsOf = (installations: readonly Installation[]) =>
    ALTERNATIVES.format(installations.map((installation) => installation.label));
  const commands = ALTERNATIVES.formatToParts(
    packageManagers.map((installation) => installation.command),
  ).map((part): PhrasingContent =>
    part.type === "element"
      ? { type: "inlineCode", value: part.value }
      : { type: "text", value: part.value },
  );
  return {
    type: "paragraph",
    children: [
      { type: "text", value: "Commands on this page run " },
      { type: "inlineCode", value: PROGRAM_NAME },
      {
        type: "text",
        value: ` as installed with ${labelsOf(direct)}. If you installed Sprawl Guard with ${labelsOf(packageManagers)}, replace `,
      },
      { type: "inlineCode", value: PROGRAM_NAME },
      { type: "text", value: " with " },
      ...commands,
      { type: "text", value: "." },
    ],
  };
}
