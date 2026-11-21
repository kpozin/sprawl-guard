import type { APIRoute, GetStaticPaths } from "astro";
import { getCollection, type CollectionEntry } from "astro:content";

import { formatMarkdownRenderError } from "../markdown/render-errors.ts";
import { renderDocumentationMarkdown, type DocumentationEntrySource } from "../markdown/render.ts";
import { createDocumentationRoutes, isGenerated404EntryId } from "../markdown/routes.ts";

interface MarkdownPageProps {
  readonly markdown: string;
}

export const getStaticPaths: GetStaticPaths = async () => {
  const entries = (await getCollection("docs")).filter(
    (entry) => !entry.data.draft && !isGenerated404EntryId(entry.id),
  );
  const routesResult = createDocumentationRoutes(entries.map((entry) => entry.id));
  if (!routesResult.ok) {
    throw new Error(`Cannot create documentation routes: ${routesResult.error.kind}`);
  }

  const entriesById = new Map(entries.map((entry) => [entry.id, entry]));
  return routesResult.value.entries.map((route) => {
    const entry = entriesById.get(route.entryId);
    if (entry === undefined) {
      throw new Error(`Missing documentation entry for route: ${route.entryId}`);
    }
    const rendered = renderDocumentationMarkdown(markdownSource(entry), routesResult.value);
    if (!rendered.ok) throw new Error(formatMarkdownRenderError(rendered.error));
    return {
      params: { slug: route.routeSlug },
      props: { markdown: rendered.value } satisfies MarkdownPageProps,
    };
  });
};

export const GET: APIRoute<MarkdownPageProps> = ({ props }) =>
  new Response(props.markdown, {
    headers: {
      "content-type": "text/markdown; charset=utf-8",
    },
  });

function markdownSource(entry: CollectionEntry<"docs">): DocumentationEntrySource {
  const hero = entry.data.hero;
  return {
    id: entry.id,
    title: entry.data.title,
    description: entry.data.description ?? "",
    body: entry.body ?? "",
    ...(hero && typeof hero === "object" && typeof hero.tagline === "string"
      ? {
          hero: {
            tagline: hero.tagline,
            actions: (hero.actions ?? []).map((action) => ({
              text: action.text,
              link: action.link,
            })),
          },
        }
      : {}),
  };
}
