import {
  createDocumentationRoutes,
  isGenerated404EntryId,
  parseDocsEntryId,
  type DocsEntryId,
  type DocumentationRouteError,
  type DocumentationRoutes,
  type Result,
} from "../markdown/routes.ts";

export type DocumentationPageDiscoveryError =
  | { readonly kind: "invalid_page_pathname"; readonly pathname: string }
  | DocumentationRouteError;

/** Parses Astro's generated HTML page list into public documentation routes. */
export function documentationRoutesFromPagePathnames(
  pagePathnames: readonly string[],
): Result<DocumentationRoutes, DocumentationPageDiscoveryError> {
  const entryIds: DocsEntryId[] = [];

  for (const pathname of pagePathnames) {
    if (pathname === "404.html" || pathname === "/404.html") continue;

    const entryId =
      pathname === "" || pathname === "/"
        ? "index"
        : pathname.replace(/^\//u, "").replace(/\/$/u, "");
    if (isGenerated404EntryId(entryId)) continue;
    const parsedEntryId = parseDocsEntryId(entryId);
    if (!parsedEntryId.ok) {
      return { ok: false, error: { kind: "invalid_page_pathname", pathname } };
    }
    entryIds.push(parsedEntryId.value);
  }

  return createDocumentationRoutes(entryIds.sort());
}
