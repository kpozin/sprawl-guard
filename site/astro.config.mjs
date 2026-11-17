import { defineConfig, passthroughImageService } from "astro/config";
import starlight from "@astrojs/starlight";
import cliCommandTabs from "./src/integrations/cli-command-tabs.ts";
import markdownOutputCheck from "./src/integrations/markdown-output-check.ts";

const site = "https://kpozin.github.io";
const base = "/sprawl-guard";

/**
 * The image that link previews show, rendered from `src/assets/social-card.svg` by
 * `just sprawl-guard-public-site-social-card`. Open Graph needs an absolute URL.
 */
const socialCard = {
  url: new URL(`${base}/social-card.png`, site).href,
  width: 1200,
  height: 630,
  alt: "Sprawl Guard: keep source files and directories within explicit size limits.",
};

export default defineConfig({
  site,
  base,
  output: "static",
  image: {
    service: passthroughImageService(),
  },
  integrations: [
    starlight({
      title: "Sprawl Guard",
      logo: {
        src: "./src/assets/sprawl-guard-mark.svg",
        alt: "",
      },
      head: [
        { tag: "meta", attrs: { property: "og:image", content: socialCard.url } },
        { tag: "meta", attrs: { property: "og:image:width", content: String(socialCard.width) } },
        { tag: "meta", attrs: { property: "og:image:height", content: String(socialCard.height) } },
        { tag: "meta", attrs: { property: "og:image:alt", content: socialCard.alt } },
      ],
      customCss: ["./src/styles/theme.css"],
      routeMiddleware: "./src/starlight-route-data.ts",
      components: {
        Footer: "./src/components/Footer.astro",
        SiteTitle: "./src/components/SiteTitle.astro",
        TableOfContents: "./src/components/TableOfContents.astro",
      },
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/kpozin/sprawl-guard",
        },
        {
          icon: "npm",
          label: "npm",
          href: "https://www.npmjs.com/package/@kpozin/sprawl-guard",
        },
        {
          icon: "seti:rust",
          label: "crates.io",
          href: "https://crates.io/crates/sprawl-guard",
        },
      ],
      sidebar: [
        {
          label: "Learn",
          items: [
            { label: "Get started", slug: "quick-start" },
            { label: "Run in CI", slug: "run-in-ci" },
            { label: "Use with Bazel", slug: "use-with-bazel" },
            { label: "Work with the ratchet", slug: "ratchet" },
            { label: "Configure checks", slug: "configure-checks" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "CLI and output", slug: "reference/cli-and-output" },
            { label: "Configuration", slug: "reference/configuration" },
            { label: "How checks work", slug: "reference/how-checks-work" },
          ],
        },
      ],
    }),
    cliCommandTabs(),
    markdownOutputCheck(),
  ],
});
