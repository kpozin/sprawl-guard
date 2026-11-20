import type { List, Paragraph } from "mdast";
import type { MdastNode } from "satteri";

import {
  INSTALLATIONS,
  inlineCommandArguments,
  parseCodeBlock,
  PROGRAM_NAME,
} from "./cli-commands.ts";

/** Joins alternatives as English prose does, as in "Cargo or Homebrew". */
const ALTERNATIVES = new Intl.ListFormat("en", { type: "disjunction" });

/**
 * Reports whether `nodes`, however deeply nested, hold a command that readers run differently per
 * installation: a code block flagged as commands, or a command written as inline code.
 */
export function containsCliCommands(nodes: readonly MdastNode[]): boolean {
  return nodes.some((node) => {
    if (node.type === "code") {
      const parsed = parseCodeBlock(node);
      return parsed.ok && parsed.value.kind === "cli_commands";
    }
    if (node.type === "inlineCode") return inlineCommandArguments(node.value) !== undefined;
    return "children" in node && Array.isArray(node.children) && containsCliCommands(node.children);
  });
}

/**
 * Tells readers of the agent-readable Markdown how to run a page's commands when they installed
 * Sprawl Guard through a package manager, since that output has no tabs.
 */
export function installationNote(): [Paragraph, List] {
  const direct = INSTALLATIONS.filter((installation) => installation.command === PROGRAM_NAME);
  const packageManagers = INSTALLATIONS.filter(
    (installation) => installation.command !== PROGRAM_NAME,
  );
  const directLabels = ALTERNATIVES.format(direct.map((installation) => installation.label));
  return [
    {
      type: "paragraph",
      children: [
        { type: "text", value: "Commands on this page run " },
        { type: "inlineCode", value: PROGRAM_NAME },
        {
          type: "text",
          value: ` as installed with ${directLabels}. If you installed Sprawl Guard with a package manager, run it this way instead:`,
        },
      ],
    },
    {
      type: "list",
      ordered: false,
      spread: false,
      children: packageManagers.map(({ label, command }) => ({
        type: "listItem",
        children: [
          {
            type: "paragraph",
            children: [
              { type: "text", value: `${label}: ` },
              { type: "inlineCode", value: command },
            ],
          },
        ],
      })),
    },
  ];
}
