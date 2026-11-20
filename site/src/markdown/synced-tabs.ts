// Zod Mini, because the page script that switches variants also parses with these schemas, and
// full Zod would add about 19 KB, gzipped, to every page.
import * as z from "zod/mini";

/** The key that ties Starlight tab groups together, so that picking a tab in one picks it in all. */
export const syncKeySchema = z.string().check(z.minLength(1)).brand<"SyncKey">();
export type SyncKey = z.infer<typeof syncKeySchema>;

/** A tab's label, which also names the reader's pick across the groups that share a sync key. */
export const tabLabelSchema = z.string().check(z.minLength(1)).brand<"TabLabel">();
export type TabLabel = z.infer<typeof tabLabelSchema>;
