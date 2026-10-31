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
    alias: {
      "@xenosite/xpict": resolve(jsRoot, "src/index.ts"),
    },
  },
  build: {
    outDir: resolve(jsRoot, "dist-review"),
    emptyOutDir: true,
  },
});
