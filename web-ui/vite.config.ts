/// <reference types="vitest" />
import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// The built files are served by the API Server over HTTPS (DEC-045).
export default defineConfig({
  plugins: [preact()],
  build: { outDir: "dist", sourcemap: false, target: "es2020" },
  test: { environment: "jsdom", globals: true, setupFiles: ["src/test-setup.ts"] },
});
