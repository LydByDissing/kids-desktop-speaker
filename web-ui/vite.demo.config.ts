import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// The review page (src/demo): the app against the mocked speaker, built into
// demo-dist and folded into one HTML file by scripts/demo-page.mjs.
export default defineConfig({
  plugins: [preact()],
  build: {
    outDir: "demo-dist",
    target: "es2020",
    assetsInlineLimit: 100_000,
    rollupOptions: { input: "demo.html", output: { inlineDynamicImports: true } },
  },
});
