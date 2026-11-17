import { isSatteriProcessor } from "@astrojs/markdown-satteri";
import type { AstroIntegration } from "astro";
import type { Code } from "mdast";
import type { MdastPluginDefinition, MdxJsxFlowElement, PluginFactoryContext } from "satteri";

import {
  CLI_COMMANDS_FLAG,
  INSTALLATIONS,
  PACKAGE_MANAGER_SYNC_KEY,
  commandsFor,
  formatCliCommandBlockError,
  parseCodeBlock,
  type CliCommandBlock,
} from "../markdown/cli-commands.ts";

/** Local names for the tab components, chosen so they can't clash with a page's own imports. */
const TABS_COMPONENT = "SprawlGuardCliTabs";
const TAB_ITEM_COMPONENT = "SprawlGuardCliTabItem";

/** The import that a page needs once the plugin has added tabs to it. */
const TABS_IMPORT = `import { Tabs as ${TABS_COMPONENT}, TabItem as ${TAB_ITEM_COMPONENT} } from "@astrojs/starlight/components";`;

/**
 * Shows each code block flagged as Sprawl Guard commands as synced tabs, one per installation, so
 * readers copy the command that runs the CLI the way they installed it.
 */
export default function cliCommandTabs(): AstroIntegration {
  return {
    name: "sprawl-guard-cli-command-tabs",
    hooks: {
      "astro:config:setup": ({ config }) => {
        const processor = config.markdown.processor;
        if (!isSatteriProcessor(processor)) {
          throw new Error(
            `CLI command tabs need the satteri Markdown processor, not ${processor.name}`,
          );
        }
        processor.options.mdastPlugins.push(cliCommandTabsPlugin);
      },
    },
  };
}

/** Creates the plugin for one document. Tabs are MDX components, so plain Markdown can't have them. */
export function cliCommandTabsPlugin({ fileURL, sourceFormat }: PluginFactoryContext) {
  const page = fileURL?.pathname ?? "a Markdown document";
  let addedTabs = false;
  return {
    name: "sprawl-guard-cli-command-tabs",
    code(node) {
      const parsed = parseCodeBlock(node);
      if (!parsed.ok) throw new Error(`${page}: ${formatCliCommandBlockError(parsed.error)}`);
      if (parsed.value.kind !== "cli_commands") return undefined;
      if (sourceFormat !== "mdx") {
        throw new Error(`${page}: a ${CLI_COMMANDS_FLAG} code block needs an .mdx page`);
      }
      addedTabs = true;
      return commandTabs(node, parsed.value.block);
    },
    after(root, context) {
      if (addedTabs) context.prependChild(root, { type: "mdxjsEsm", value: TABS_IMPORT });
    },
  } satisfies MdastPluginDefinition;
}

/** Builds the tabs for one command block, each holding the block's commands for one installation. */
function commandTabs(node: Code, block: CliCommandBlock): MdxJsxFlowElement {
  return {
    type: "mdxJsxFlowElement",
    name: TABS_COMPONENT,
    attributes: [{ type: "mdxJsxAttribute", name: "syncKey", value: PACKAGE_MANAGER_SYNC_KEY }],
    children: INSTALLATIONS.map((installation) => ({
      type: "mdxJsxFlowElement",
      name: TAB_ITEM_COMPONENT,
      attributes: [{ type: "mdxJsxAttribute", name: "label", value: installation.label }],
      children: [
        {
          type: "code",
          lang: node.lang,
          meta: block.meta,
          value: commandsFor(block, installation),
        },
      ],
    })),
  };
}
