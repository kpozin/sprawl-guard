/**
 * Shows, in each group of synced variants that `src/markdown/synced-variants.ts` builds, the
 * variant for the tab the reader picked in Starlight tabs with the same sync key. The group shows
 * the variant of the page's selected tab when the reader hasn't picked one. On a page without
 * such tabs, it shows its content as written, which has no label.
 */

import {
  syncKeySchema,
  tabLabelSchema,
  type SyncKey,
  type TabLabel,
} from "../markdown/synced-tabs.ts";

/**
 * The prefix of the local storage key under which Starlight keeps each sync key's picked label.
 * Starlight doesn't document it, so keep it in sync with Starlight's `Tabs` component on upgrades.
 */
const STARLIGHT_STORAGE_PREFIX = "starlight-synced-tabs__";

/**
 * Shows the variant labelled `label` in every group synced by `syncKey`, or else the content as
 * written.
 */
function showVariants(syncKey: SyncKey, label: TabLabel | undefined): void {
  for (const group of document.querySelectorAll(
    `.synced-variants[data-sync-key="${CSS.escape(syncKey)}"]`,
  )) {
    const variants = [...group.children].filter((child) => child instanceof HTMLElement);
    const shown =
      variants.find((variant) => label !== undefined && variant.dataset.syncLabel === label) ??
      variants.find((variant) => variant.dataset.syncLabel === undefined);
    for (const variant of variants) variant.hidden = variant !== shown;
  }
}

/** Reads the label that Starlight stored for `syncKey`, if storage is available and holds one. */
function storedLabel(syncKey: SyncKey): TabLabel | undefined {
  try {
    return tabLabelSchema.safeParse(localStorage.getItem(STARLIGHT_STORAGE_PREFIX + syncKey)).data;
  } catch {
    return undefined;
  }
}

/** Reads the label of the selected tab within `container`, if it has one. */
function selectedLabel(container: ParentNode | null | undefined): TabLabel | undefined {
  const selected = container?.querySelector('[role="tab"][aria-selected="true"]');
  return tabLabelSchema.safeParse(selected?.textContent?.trim()).data;
}

const syncKeys = new Set(
  [...document.querySelectorAll<HTMLElement>(".synced-variants")].flatMap(
    (group) => syncKeySchema.safeParse(group.dataset.syncKey).data ?? [],
  ),
);
// Without a stored pick, the page's first tabs with the same sync key show which variant readers
// see in them, so the text matches.
for (const syncKey of syncKeys) {
  const pageTabs = document.querySelector(`starlight-tabs[data-sync-key="${CSS.escape(syncKey)}"]`);
  showVariants(syncKey, storedLabel(syncKey) ?? selectedLabel(pageTabs));
}

// Starlight selects a tab on click or arrow key without announcing it, so after either event in a
// tab list, that list's selected tab gives the reader's pick. Events in tab panels, such as copying
// a command, select nothing.
for (const type of ["click", "keydown"]) {
  document.addEventListener(type, (event) => {
    if (!(event.target instanceof Element)) return;
    const tabList = event.target.closest('[role="tablist"]');
    const tabs = tabList?.closest<HTMLElement>("starlight-tabs[data-sync-key]");
    const syncKey = syncKeySchema.safeParse(tabs?.dataset.syncKey).data;
    if (syncKey === undefined || !syncKeys.has(syncKey)) return;
    showVariants(syncKey, selectedLabel(tabList));
  });
}
