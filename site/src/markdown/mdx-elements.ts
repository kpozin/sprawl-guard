import { parse, type ImportDeclaration } from "acorn";
import type { MdastNode, MdxJsxFlowElement, MdxJsxTextElement } from "satteri";

import { unsupported, type MarkdownRenderError } from "./render-errors.ts";
import type { DocsEntryId, Result } from "./routes.ts";

/** A JSX element in MDX, either on its own lines or inside a paragraph. */
type MdxJsxElement = MdxJsxFlowElement | MdxJsxTextElement;

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

/**
 * Maps each component an entry imports to its kind. Fails on any other module statement, since
 * the Markdown export can't run code.
 */
export function collectComponentBindings(
  nodes: readonly MdastNode[],
  entryId: DocsEntryId,
): Result<ReadonlyMap<string, ComponentKind>, MarkdownRenderError> {
  const bindings = new Map<string, ComponentKind>();
  for (const node of nodes) {
    if (node.type !== "mdxjsEsm") continue;

    let program;
    try {
      program = parse(node.value, { ecmaVersion: "latest", sourceType: "module" });
    } catch {
      return unsupported(entryId, "malformed MDX module declaration");
    }

    for (const statement of program.body) {
      if (statement.type !== "ImportDeclaration") {
        return unsupported(entryId, "MDX export");
      }
      const result = collectImportBindings(statement, entryId, bindings);
      if (!result.ok) return result;
    }
  }
  return { ok: true, value: bindings };
}

function collectImportBindings(
  declaration: ImportDeclaration,
  entryId: DocsEntryId,
  bindings: Map<string, ComponentKind>,
): Result<void, MarkdownRenderError> {
  const source = String(declaration.source.value);
  for (const specifier of declaration.specifiers) {
    const kind = componentImportKind(source, specifier, entryId);
    if (kind === undefined) {
      return unsupported(entryId, `${specifier.local.name} imported from ${source}`);
    }
    if (bindings.has(specifier.local.name)) {
      return unsupported(entryId, `duplicate import binding ${specifier.local.name}`);
    }
    bindings.set(specifier.local.name, kind);
  }

  if (declaration.specifiers.length === 0) {
    return unsupported(entryId, `side-effect import from ${source}`);
  }
  return { ok: true, value: undefined };
}

/** Identifies the components whose content can be exported to Markdown. */
function componentImportKind(
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
export function requiredStringAttribute(node: MdxJsxElement, name: string): string | undefined {
  const matches = node.attributes.filter(
    (attribute) => attribute.type === "mdxJsxAttribute" && attribute.name === name,
  );
  const [match, ...others] = matches;
  if (others.length > 0) return undefined;
  return typeof match?.value === "string" ? match.value : undefined;
}

/**
 * Writes an empty anchor that gives a link a target inside prose, such as the note the homepage
 * tagline links to, as HTML that the Markdown export keeps. Returns `undefined` unless the anchor
 * has only a literal `id` and no content.
 */
export function anchorTargetHtml(node: MdxJsxTextElement): string | undefined {
  const id = requiredStringAttribute(node, "id");
  if (!id || !hasOnlyStringAttributes(node, ["id"]) || node.children.length > 0) return undefined;
  return `<a id="${id.replaceAll("&", "&amp;").replaceAll('"', "&quot;")}"></a>`;
}

/** Accepts only literal string attributes with one of the allowed names. */
export function hasOnlyStringAttributes(
  node: MdxJsxElement,
  allowedNames: readonly string[],
): boolean {
  return node.attributes.every(
    (attribute) =>
      attribute.type === "mdxJsxAttribute" &&
      allowedNames.includes(attribute.name) &&
      typeof attribute.value === "string",
  );
}
