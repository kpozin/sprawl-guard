import { defineConfig } from "vite-plus";

export default defineConfig({
  pack: {
    entry: "src/bin/sprawl-guard.ts",
    target: "node22",
    outputOptions: {
      entryFileNames: "bin/sprawl-guard.js",
      chunkFileNames: "lib/[name].js",
    },
    deps: { onlyBundle: ["better-result"], onlyImport: [] },
    copy: { from: "node_modules/better-result/LICENSE", rename: "LICENSE-better-result" },
  },
});
