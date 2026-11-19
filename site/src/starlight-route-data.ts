import { defineRouteMiddleware } from "@astrojs/starlight/route-data";

import { createDocumentationRoutes, isGenerated404EntryId } from "./markdown/routes.ts";

/** Advertises the Markdown counterpart of each public documentation page. */
export const onRequest = defineRouteMiddleware(({ locals }) => {
  const { entry, head } = locals.starlightRoute;
  if (entry.data.draft || isGenerated404EntryId(entry.id)) return;

  const result = createDocumentationRoutes([entry.id === "" ? "index" : entry.id]);
  if (!result.ok) {
    throw new Error(`Cannot create Markdown alternate: ${result.error.kind}`);
  }
  const [route] = result.value.entries;
  if (route === undefined) {
    throw new Error("Cannot create Markdown alternate: no route for the entry");
  }

  head.push({
    tag: "link",
    attrs: {
      rel: "alternate",
      type: "text/markdown",
      href: route.markdownPath,
    },
  });
});
