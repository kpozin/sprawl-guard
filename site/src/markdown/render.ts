import type {
  BlockContent,
  Code,
  List,
  ListItem,
  Paragraph,
  PhrasingContent,
  Root,
  RootContent,
} from "mdast";
import { gfmToMarkdown } from "mdast-util-gfm";
import { toMarkdown } from "mdast-util-to-markdown";
import { mdxToMdast, type MdastNode, type MdxJsxFlowElement } from "satteri";

import { containsCliCommands, installationNote } from "./cli-command-nodes.ts";
import { parseCodeBlock } from "./cli-commands.ts";
import { cliCommandReferenceNodes } from "./cli-reference-nodes.ts";
import { findCliReferenceCommand } from "./cli-reference.ts";
import {
  anchorTargetHtml,
  collectComponentBindings,
  hasOnlyStringAttributes,
  requiredStringAttribute,
  type ComponentKind,
} from "./mdx-elements.ts";
import {
  invalidEntryId,
  malformed,
  unsupported,
  type MarkdownRenderError,
} from "./render-errors.ts";
import {
  parseDocsEntryId,
  rewriteDocumentationLink,
  type DocsEntryId,
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
      // A hero's tagline restates the description, so a page shows only one of them.
      ...(source.hero === undefined
        ? [paragraph(source.description)]
        : heroNodes(source.hero, context)),
      ...(containsCliCommands(tree.children) ? installationNote() : []),
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
      return node.name === "details"
        ? transformDetails(node, context)
        : transformComponent(node, context);
    case "mdxJsxTextElement": {
      if (node.name !== "a") return unsupported(context.entryId, node.name ?? "MDX fragment");
      const html = anchorTargetHtml(node);
      if (html === undefined) {
        return malformed(context.entryId, "a", "expected an empty anchor with only an id");
      }
      return { ok: true, value: [{ type: "html", value: html }] };
    }
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

/**
 * Expands a collapsed section, since agents can't open one: its summary becomes a bold paragraph
 * ahead of its content.
 */
function transformDetails(
  node: MdxJsxFlowElement,
  context: TransformContext,
): Result<RootContent[], MarkdownRenderError> {
  const [summary, ...content] = node.children;
  if (
    node.attributes.length !== 0 ||
    summary?.type !== "mdxJsxFlowElement" ||
    summary.name !== "summary" ||
    summary.attributes.length !== 0
  ) {
    return malformed(context.entryId, "details", "expected no attributes and a summary first");
  }
  const title = transformChildren(summary.children, context);
  if (!title.ok) return title;
  const titleText = title.value.filter(isPhrasing);
  if (titleText.length === 0 || titleText.length !== title.value.length) {
    return malformed(context.entryId, "summary", "expected one line of text");
  }
  const rest = transformChildren(content, context);
  if (!rest.ok) return rest;
  return {
    ok: true,
    value: [
      { type: "paragraph", children: [{ type: "strong", children: titleText }] },
      ...rest.value,
    ],
  };
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

  const items: ListItem[] = [];
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
    items.push({
      type: "listItem",
      children: [paragraphStrong(label), ...(content.value as BlockContent[])],
    });
  }
  // Each tab's content nests under its label, so readers can tell where the tabs end.
  return { ok: true, value: [{ type: "list", ordered: false, spread: true, children: items }] };
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

function heroNodes(hero: MarkdownHero, context: TransformContext): RootContent[] {
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
  return [paragraphWithChildren([{ type: "html", value: hero.tagline }]), actions];
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

/** The node types that make up a line of text, listed in full so the compiler flags new ones. */
const PHRASING_TYPES: Readonly<Record<PhrasingContent["type"], true>> = {
  break: true,
  delete: true,
  emphasis: true,
  footnoteReference: true,
  html: true,
  image: true,
  imageReference: true,
  inlineCode: true,
  inlineMath: true,
  link: true,
  linkReference: true,
  mdxJsxTextElement: true,
  mdxTextExpression: true,
  strong: true,
  subscript: true,
  superscript: true,
  text: true,
  textDirective: true,
};

function isPhrasing(node: RootContent): node is PhrasingContent {
  return Object.hasOwn(PHRASING_TYPES, node.type);
}
