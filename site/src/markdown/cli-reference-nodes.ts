import type { PhrasingContent, RootContent, Strong, Table, TableCell, TableRow } from "mdast";

import { inlineCodeSegments, type CliReferenceCommand } from "./cli-reference.ts";

/** Converts one parsed CLI command into the MDAST nodes used by Markdown pages. */
export function cliCommandReferenceNodes(command: CliReferenceCommand): RootContent[] {
  const rows: TableRow[] = [
    tableRow([textCell("Argument or option"), textCell("Meaning")]),
    ...command.arguments.map((argument) =>
      tableRow([
        { type: "tableCell", children: [{ type: "inlineCode", value: argument.syntax }] },
        { type: "tableCell", children: argumentMeaning(argument) },
      ]),
    ),
  ];
  const table: Table = { type: "table", align: [null, null], children: rows };
  return [{ type: "paragraph", children: [{ type: "inlineCode", value: command.usage }] }, table];
}

function argumentMeaning(argument: CliReferenceCommand["arguments"][number]): PhrasingContent[] {
  const children = argument.help === null ? [] : inlineNodes(argument.help);
  if (argument.default_values.length > 0) {
    appendLabeledInlineCode(children, "Default:", argument.default_values.join(" "));
  }
  if (argument.conflicts_with.length > 0) {
    appendLabeledInlineCode(children, "Conflicts with:", argument.conflicts_with.join(", "));
  }
  if (argument.possible_values.length > 0) {
    appendSeparator(children);
    children.push(strong("Allowed values:"));
    for (const [index, value] of argument.possible_values.entries()) {
      children.push({ type: "text", value: index === 0 ? " " : "; " });
      children.push({ type: "inlineCode", value: value.name });
      if (value.help !== null) {
        children.push({ type: "text", value: " — " }, ...inlineNodes(value.help));
      }
    }
  }
  return children;
}

function appendLabeledInlineCode(children: PhrasingContent[], label: string, value: string): void {
  appendSeparator(children);
  children.push(strong(label), { type: "text", value: " " }, { type: "inlineCode", value });
}

function appendSeparator(children: PhrasingContent[]): void {
  if (children.length > 0) children.push({ type: "text", value: " " });
}

function inlineNodes(value: string): PhrasingContent[] {
  return inlineCodeSegments(value).map((segment) =>
    segment.kind === "code"
      ? { type: "inlineCode", value: segment.value }
      : { type: "text", value: segment.value },
  );
}

function strong(value: string): Strong {
  return { type: "strong", children: [{ type: "text", value }] };
}

function textCell(value: string): TableCell {
  return { type: "tableCell", children: [{ type: "text", value }] };
}

function tableRow(children: TableCell[]): TableRow {
  return { type: "tableRow", children };
}
