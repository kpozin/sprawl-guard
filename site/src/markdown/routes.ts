const SITE_BASE_PATH = "/sprawl-guard";
const DOCS_ENTRY_ID_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*(?:\/[a-z0-9]+(?:-[a-z0-9]+)*)*$/u;

declare const docsEntryIdBrand: unique symbol;
declare const documentationHtmlPathBrand: unique symbol;
declare const documentationMarkdownPathBrand: unique symbol;
declare const markdownRouteSlugBrand: unique symbol;

export type DocsEntryId = string & { readonly [docsEntryIdBrand]: true };
export type DocumentationHtmlPath = string & {
  readonly [documentationHtmlPathBrand]: true;
};
export type DocumentationMarkdownPath = string & {
  readonly [documentationMarkdownPathBrand]: true;
};
export type MarkdownRouteSlug = string & {
  readonly [markdownRouteSlugBrand]: true;
};

export type Result<T, E> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };

export interface DocumentationRoute {
  readonly entryId: DocsEntryId;
  readonly htmlPath: DocumentationHtmlPath;
  readonly markdownPath: DocumentationMarkdownPath;
  readonly routeSlug: MarkdownRouteSlug;
}

export interface DocumentationRoutes {
  readonly entries: readonly DocumentationRoute[];
  readonly byEntryId: ReadonlyMap<DocsEntryId, DocumentationRoute>;
  readonly byHtmlPath: ReadonlyMap<DocumentationHtmlPath, DocumentationRoute>;
}

export type DocumentationRouteError =
  | { readonly kind: "invalid_entry_id"; readonly entryId: string }
  | { readonly kind: "route_collision"; readonly path: string };

/** Parses collection entry IDs and constructs their public HTML and Markdown routes. */
export function createDocumentationRoutes(
  rawEntryIds: readonly string[],
): Result<DocumentationRoutes, DocumentationRouteError> {
  const entries: DocumentationRoute[] = [];
  const byEntryId = new Map<DocsEntryId, DocumentationRoute>();
  const byHtmlPath = new Map<DocumentationHtmlPath, DocumentationRoute>();

  for (const rawEntryId of rawEntryIds) {
    const parsedEntryId = parseDocsEntryId(rawEntryId);
    if (!parsedEntryId.ok) return parsedEntryId;

    const entry = routeForEntryId(parsedEntryId.value);
    if (byEntryId.has(entry.entryId)) {
      return { ok: false, error: { kind: "route_collision", path: entry.htmlPath } };
    }
    if (byHtmlPath.has(entry.htmlPath)) {
      return { ok: false, error: { kind: "route_collision", path: entry.htmlPath } };
    }
    entries.push(entry);
    byEntryId.set(entry.entryId, entry);
    byHtmlPath.set(entry.htmlPath, entry);
  }

  return { ok: true, value: { entries, byEntryId, byHtmlPath } };
}

/** Parses an Astro content collection entry ID. */
export function parseDocsEntryId(rawEntryId: string): Result<DocsEntryId, DocumentationRouteError> {
  if (!DOCS_ENTRY_ID_PATTERN.test(rawEntryId)) {
    return {
      ok: false,
      error: { kind: "invalid_entry_id", entryId: rawEntryId },
    };
  }
  return { ok: true, value: rawEntryId as DocsEntryId };
}

/** Rewrites a link to a public documentation page when it has a Markdown counterpart. */
export function rewriteDocumentationLink(
  link: string,
  currentEntryId: DocsEntryId,
  routes: DocumentationRoutes,
): string {
  if (link.startsWith("#") || link.startsWith("//") || /^[a-z][a-z0-9+.-]*:/iu.test(link)) {
    return link;
  }

  const currentRoute = routes.byEntryId.get(currentEntryId);
  if (currentRoute === undefined) return link;

  const resolved = new URL(link, `https://docs.invalid${currentRoute.htmlPath}`);
  const normalizedPath = normalizeDocumentationHtmlPath(resolved.pathname);
  const target = routes.byHtmlPath.get(normalizedPath);
  if (target === undefined) return link;
  return `${target.markdownPath}${resolved.search}${resolved.hash}`;
}

/** Identifies Starlight's generated fallback page, which is not authored documentation. */
export function isGenerated404EntryId(entryId: string): boolean {
  return entryId === "404" || entryId.endsWith("/404");
}

function routeForEntryId(entryId: DocsEntryId): DocumentationRoute {
  const isIndex = entryId === "index";
  return {
    entryId,
    htmlPath: `${SITE_BASE_PATH}${isIndex ? "/" : `/${entryId}/`}` as DocumentationHtmlPath,
    markdownPath: `${SITE_BASE_PATH}/${entryId}.md` as DocumentationMarkdownPath,
    routeSlug: `${entryId}` as MarkdownRouteSlug,
  };
}

function normalizeDocumentationHtmlPath(path: string): DocumentationHtmlPath {
  if (path === `${SITE_BASE_PATH}`) return `${SITE_BASE_PATH}/` as DocumentationHtmlPath;
  return (path.endsWith("/") ? path : `${path}/`) as DocumentationHtmlPath;
}
