import { parse, type ImportDeclaration } from "acorn";
import type { Code, List, ListItem, Paragraph, PhrasingContent, Root, RootContent } from "mdast";
import { gfmToMarkdown } from "mdast-util-gfm";
import { toMarkdown } from "mdast-util-to-markdown";
import { mdxToMdast, type MdastNode, type MdxJsxFlowElement } from "satteri";

import { containsCliCommandBlock, installationNote } from "./cli-command-nodes.ts";
import {
  formatCliCommandBlockError,
  parseCodeBlock,
  type CliCommandBlockError,
} from "./cli-commands.ts";
import { cliCommandReferenceNodes } from "./cli-reference-nodes.ts";
import { findCliReferenceCommand, type CliReferenceError } from "./cli-reference.ts";
import {
  componentImportKind,
  hasOnlyStringAttributes,
  requiredStringAttribute,
  type ComponentKind,
} from "./mdx-elements.ts";
import {
  parseDocsEntryId,
  rewriteDocumentationLink,
  type DocsEntryId,
  type DocumentationRouteError,
  type DocumentationRoutes,
  type Result,
} from "./routes.ts";

interface HeroAction {
  readonly text: string;
  readonly link: string;
}

interface MarkdownHero {
  readonly tagline: string;
  readonly actions: readonly HeroAction[];
}

export interface DocumentationEntrySource {
  readonly id: string;
  readonly title: string;
  readonly description: string;
  readonly body: string;
  readonly hero?: MarkdownHero;
}

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

interface TransformContext {
  readonly entryId: DocsEntryId;
  readonly routes: DocumentationRoutes;
  readonly components: ReadonlyMap<string, ComponentKind>;
}

/** Converts one Starlight content entry into its agent-readable Markdown form. */
export function renderDocumentationMarkdown(
  source: DocumentationEntrySource,
  routes: DocumentationRoutes,
): Result<string, MarkdownRenderError> {
  const parsedEntryId = parseDocsEntryId(source.id);
  if (!parsedEntryId.ok) return invalidEntryId(parsedEntryId.error);
  const entryId = parsedEntryId.value;
  if (!routes.byEntryId.has(entryId)) {
    return { ok: false, error: { kind: "missing_documentation_route", entryId } };
  }

  const tree = mdxToMdast(source.body);
  if (tree.type !== "root") {
    return unsupported(entryId, tree.type);
  }

  const components = collectComponentBindings(tree.children, entryId);
  if (!components.ok) return components;
  const context = { entryId, routes, components: components.value };
  const transformedBody = transformChildren(tree.children, context);
  if (!transformedBody.ok) return transformedBody;

  const root: Root = {
    type: "root",
    children: [
      { type: "heading", depth: 1, children: [{ type: "text", value: source.title }] },
      paragraph(source.description),
      ...heroNodes(source.hero, context),
      ...(containsCliCommandBlock(tree.children) ? [installationNote()] : []),
      ...transformedBody.value,
    ],
  };
  return {
    ok: true,
    value: toMarkdown(root, {
      bullet: "-",
      extensions: [gfmToMarkdown()],
      listItemIndent: "one",
    }),
  };
}

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

function collectComponentBindings(
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

function transformChildren(
  children: readonly MdastNode[],
  context: TransformContext,
): Result<RootContent[], MarkdownRenderError> {
  const transformed: RootContent[] = [];
  for (const child of children) {
    const result = transformNode(child, context);
    if (!result.ok) return result;
    transformed.push(...result.value);
  }
  return { ok: true, value: transformed };
}

function transformNode(
  node: MdastNode,
  context: TransformContext,
): Result<RootContent[], MarkdownRenderError> {
  switch (node.type) {
    case "mdxjsEsm":
      return { ok: true, value: [] };
    case "mdxFlowExpression":
    case "mdxTextExpression":
      return unsupported(context.entryId, "MDX expression");
    case "mdxJsxFlowElement":
      return transformComponent(node, context);
    case "mdxJsxTextElement":
      return unsupported(context.entryId, node.name ?? "MDX fragment");
    case "yaml":
    case "toml":
      return { ok: true, value: [] };
    case "code":
      return transformCode(node, context);
    default:
      return transformOrdinaryNode(node, context);
  }
}

function transformOrdinaryNode(
  node: MdastNode,
  context: TransformContext,
): Result<RootContent[], MarkdownRenderError> {
  const link =
    node.type === "link"
      ? {
          ...node,
          url: rewriteDocumentationLink(node.url, context.entryId, context.routes),
        }
      : node.type === "definition"
        ? {
            ...node,
            url: rewriteDocumentationLink(node.url, context.entryId, context.routes),
          }
        : node;

  if (!("children" in link) || !Array.isArray(link.children)) {
    return { ok: true, value: [link as RootContent] };
  }

  const children = transformChildren(link.children, context);
  if (!children.ok) return children;
  return {
    ok: true,
    value: [{ ...link, children: children.value } as RootContent],
  };
}

/**
 * Keeps a code block as written, minus the flag that gives command blocks their tabs on the HTML
 * site. The flagged commands run Sprawl Guard as Cargo installs it; the page's installation note
 * explains the other forms.
 */
function transformCode(
  node: Code,
  context: TransformContext,
): Result<RootContent[], MarkdownRenderError> {
  const parsed = parseCodeBlock(node);
  if (!parsed.ok) {
    return {
      ok: false,
      error: { kind: "cli_command_block", entryId: context.entryId, cause: parsed.error },
    };
  }
  switch (parsed.value.kind) {
    case "cli_commands":
      return { ok: true, value: [{ ...node, meta: parsed.value.block.meta }] };
    case "unflagged_cli_commands":
      return {
        ok: false,
        error: { kind: "unflagged_cli_command_block", entryId: context.entryId, value: node.value },
      };
    case "other":
      return { ok: true, value: [node] };
  }
}

function transformComponent(
  node: MdxJsxFlowElement,
  context: TransformContext,
): Result<RootContent[], MarkdownRenderError> {
  const kind = node.name === null ? undefined : context.components.get(node.name);
  if (kind === undefined) return unsupported(context.entryId, node.name ?? "MDX fragment");

  switch (kind) {
    case "steps":
      if (node.attributes.length !== 0) {
        return malformed(context.entryId, "Steps", "attributes are not supported");
      }
      return transformChildren(node.children, context);
    case "tabs":
      return transformTabs(node, context);
    case "tab_item":
      return malformed(context.entryId, "TabItem", "must be a direct child of Tabs");
    case "cli_command_reference":
      return transformCliCommandReference(node, context);
    case "directory_trees":
      // Like the hero image, the illustration repeats what the text says, so agents get only the
      // text beside it.
      if (node.attributes.length !== 0) {
        return malformed(context.entryId, "DirectoryTrees", "attributes are not supported");
      }
      return transformChildren(node.children, context);
  }
}

function transformTabs(
  node: MdxJsxFlowElement,
  context: TransformContext,
): Result<RootContent[], MarkdownRenderError> {
  if (!hasOnlyStringAttributes(node, ["syncKey"])) {
    return malformed(context.entryId, "Tabs", "expected only a string syncKey attribute");
  }

  const children: RootContent[] = [];
  for (const child of node.children) {
    if (
      child.type !== "mdxJsxFlowElement" ||
      child.name === null ||
      context.components.get(child.name) !== "tab_item"
    ) {
      return malformed(context.entryId, "Tabs", "expected only TabItem children");
    }
    const label = requiredStringAttribute(child, "label");
    if (label === undefined || !hasOnlyStringAttributes(child, ["label"])) {
      return malformed(context.entryId, "TabItem", "expected one string label attribute");
    }
    const content = transformChildren(child.children, context);
    if (!content.ok) return content;
    children.push(paragraphStrong(label), ...content.value);
  }
  return { ok: true, value: children };
}

function transformCliCommandReference(
  node: MdxJsxFlowElement,
  context: TransformContext,
): Result<RootContent[], MarkdownRenderError> {
  const path = requiredStringAttribute(node, "path");
  if (
    path === undefined ||
    !hasOnlyStringAttributes(node, ["path"]) ||
    node.children.length !== 0
  ) {
    return malformed(
      context.entryId,
      "CliCommandReference",
      "expected one string path attribute and no children",
    );
  }

  const command = findCliReferenceCommand(path);
  if (!command.ok) {
    return {
      ok: false,
      error: { kind: "cli_reference", entryId: context.entryId, cause: command.error },
    };
  }
  return { ok: true, value: cliCommandReferenceNodes(command.value) };
}

function heroNodes(hero: MarkdownHero | undefined, context: TransformContext): RootContent[] {
  if (hero === undefined) return [];
  const items: ListItem[] = hero.actions.map((action) => ({
    type: "listItem",
    children: [
      paragraphWithChildren([
        {
          type: "link",
          url: rewriteDocumentationLink(action.link, context.entryId, context.routes),
          children: [{ type: "text", value: action.text }],
        },
      ]),
    ],
  }));
  const actions: List = { type: "list", ordered: false, spread: false, children: items };
  return [paragraph(hero.tagline), actions];
}

function paragraph(value: string): Paragraph {
  return paragraphWithChildren([{ type: "text", value }]);
}

function paragraphStrong(value: string): Paragraph {
  return paragraphWithChildren([{ type: "strong", children: [{ type: "text", value }] }]);
}

function paragraphWithChildren(children: PhrasingContent[]): Paragraph {
  return { type: "paragraph", children };
}

function invalidEntryId(error: DocumentationRouteError): Result<never, MarkdownRenderError> {
  if (error.kind === "invalid_entry_id") {
    return { ok: false, error: { kind: "invalid_entry_id", entryId: error.entryId } };
  }
  throw new Error(`Unexpected route error while parsing an entry ID: ${error.kind}`);
}

function unsupported(entryId: DocsEntryId, construct: string): Result<never, MarkdownRenderError> {
  return {
    ok: false,
    error: { kind: "unsupported_mdx_construct", entryId, construct },
  };
}

function malformed(
  entryId: DocsEntryId,
  component: string,
  detail: string,
): Result<never, MarkdownRenderError> {
  return {
    ok: false,
    error: { kind: "malformed_mdx_component", entryId, component, detail },
  };
}
