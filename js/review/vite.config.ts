import react from "@vitejs/plugin-react";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const here = dirname(fileURLToPath(import.meta.url));
const jsRoot = resolve(here, "..");

export default defineConfig({
  root: here,
  plugins: [react()],
  assetsInclude: ["**/*.wasm"],
  optimizeDeps: {
    exclude: ["@xenosite/xpict"],
  },
  server: {
    port: 5177,
    strictPort: true,
    fs: { allow: [jsRoot] },
  },
  resolve: {
    // Workspace `file:..` installs this repo's js/, not the registry. Alias
    // to src/ so review paints the live tree (wasm + TS) instead of a stale dist/.
    alias: {
      "@xenosite/xpict": resolve(jsRoot, "src/index.ts"),
    },
    dedupe: ["@xenosite/xpict"],
  },
  build: {
    outDir: resolve(jsRoot, "dist-review"),
    emptyOutDir: true,
  },
});
