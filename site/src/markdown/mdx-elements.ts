import type { ImportDeclaration } from "acorn";
import type { MdxJsxFlowElement } from "satteri";
import type { DocsEntryId } from "./routes.ts";

export type ComponentKind =
  | "steps"
  | "tabs"
  | "tab_item"
  | "cli_command_reference"
  | "directory_trees";

const starlightComponentKinds = new Map<string, ComponentKind>([
  ["Steps", "steps"],
  ["Tabs", "tabs"],
  ["TabItem", "tab_item"],
]);

/** Identifies the components whose content can be exported to Markdown. */
export function componentImportKind(
  source: string,
  specifier: ImportDeclaration["specifiers"][number],
  entryId: DocsEntryId,
): ComponentKind | undefined {
  if (
    source === "@astrojs/starlight/components" &&
    specifier.type === "ImportSpecifier" &&
    specifier.imported.type === "Identifier"
  ) {
    return starlightComponentKinds.get(specifier.imported.name);
  }
  if (specifier.type !== "ImportDefaultSpecifier") return undefined;
  const components = "../".repeat(entryId.split("/").length + 1) + "components/";
  if (source === `${components}CliCommandReference.astro`) return "cli_command_reference";
  if (source === `${components}DirectoryTrees.astro`) return "directory_trees";
  return undefined;
}

/** Reads a uniquely specified literal string attribute. */
export function requiredStringAttribute(node: MdxJsxFlowElement, name: string): string | undefined {
  const matches = node.attributes.filter(
    (attribute) => attribute.type === "mdxJsxAttribute" && attribute.name === name,
  );
  if (matches.length !== 1) return undefined;
  const [match] = matches;
  return typeof match.value === "string" ? match.value : undefined;
}

/** Accepts only literal string attributes with one of the allowed names. */
export function hasOnlyStringAttributes(
  node: MdxJsxFlowElement,
  allowedNames: readonly string[],
): boolean {
  return node.attributes.every(
    (attribute) =>
      attribute.type === "mdxJsxAttribute" &&
      allowedNames.includes(attribute.name) &&
      typeof attribute.value === "string",
  );
}
