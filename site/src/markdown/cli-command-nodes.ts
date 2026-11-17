import type { Paragraph, PhrasingContent } from "mdast";
import type { MdastNode } from "satteri";

import { INSTALLATIONS, type Installation, parseCodeBlock, PROGRAM_NAME } from "./cli-commands.ts";

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
    installations.map((installation) => installation.label).join(" or ");
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
      ...packageManagers.flatMap((installation, index): PhrasingContent[] => [
        ...(index === 0 ? [] : [{ type: "text" as const, value: " or " }]),
        { type: "inlineCode", value: installation.command },
      ]),
      { type: "text", value: "." },
    ],
  };
}
