import type { PhrasingContent } from "mdast";
import type { MdastTarget, MdastVisitorContext } from "satteri";

import { requiredStringAttribute } from "./mdx-elements.ts";
import type { SyncKey, TabLabel } from "./synced-tabs.ts";

/** One version of inline content, shown while readers have picked the tab named `label`. */
export interface SyncedVariant {
  readonly label: TabLabel;
  readonly content: PhrasingContent;
}

/**
 * Replaces inline `node` with versions that follow the tabs synced by `syncKey`, as a command can
 * differ per package manager. The page script `src/scripts/synced-variants.ts` shows the version
 * for the reader's picked tab, or else for the page's selected tab. On a page without such tabs,
 * `node` shows as written, since nothing on the page would say which version readers see.
 *
 * Inside a tab synced by the same key, `node` becomes that tab's version, or stays as it is when
 * the tab has none, because the tab already fixes the choice. Inside a heading, `node` stays as it
 * is, because the heading's anchor and its entry in the table of contents need one text.
 */
export function replaceWithSyncedVariants(
  node: Readonly<MdastTarget>,
  context: MdastVisitorContext,
  syncKey: SyncKey,
  variants: readonly SyncedVariant[],
): void {
  for (
    let ancestor = context.parent(node);
    ancestor !== undefined;
    ancestor = context.parent(ancestor)
  ) {
    if (ancestor.type === "heading") return;
    if (ancestor.type === "mdxJsxFlowElement" && ancestor.name === "TabItem") {
      const tabs = context.parent(ancestor);
      if (
        tabs?.type === "mdxJsxFlowElement" &&
        tabs.name === "Tabs" &&
        requiredStringAttribute(tabs, "syncKey") === syncKey
      ) {
        const label = requiredStringAttribute(ancestor, "label");
        const variant = variants.find((candidate) => candidate.label === label);
        if (variant !== undefined) context.replaceNode(node, variant.content);
        return;
      }
    }
  }
  context.replaceNode(node, {
    type: "syncedVariants",
    data: { hName: "span", hProperties: { className: ["synced-variants"], dataSyncKey: syncKey } },
    children: [
      // Search indexes only the content as written, so results don't run the variants together.
      { type: "syncedVariant", data: { hName: "span" }, children: [{ ...node }] },
      ...variants.map(({ label, content }) => ({
        type: "syncedVariant",
        data: {
          hName: "span",
          hProperties: { dataSyncLabel: label, hidden: true, dataPagefindIgnore: true },
        },
        children: [content],
      })),
    ],
  });
}
