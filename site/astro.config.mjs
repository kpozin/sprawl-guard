import { defineConfig, passthroughImageService } from "astro/config";
import starlight from "@astrojs/starlight";
import markdownOutputCheck from "./src/integrations/markdown-output-check.ts";

export default defineConfig({
  site: "https://kpozin.github.io",
  base: "/sprawl-guard",
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
      customCss: ["./src/styles/theme.css"],
      routeMiddleware: "./src/starlight-route-data.ts",
      components: {
        Footer: "./src/components/Footer.astro",
        SiteTitle: "./src/components/SiteTitle.astro",
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
            { label: "Quick start", slug: "quick-start" },
            { label: "Configure checks", slug: "configure-checks" },
            {
              label: "Adopt in an existing repository",
              slug: "adopt-existing-repository",
            },
            { label: "Run in CI", slug: "run-in-ci" },
            { label: "Clean up violations", slug: "clean-up-violations" },
            { label: "Troubleshoot", slug: "troubleshoot" },
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
    markdownOutputCheck(),
  ],
});
