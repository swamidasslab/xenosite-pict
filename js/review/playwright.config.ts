import { defineConfig } from "@playwright/test";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  testDir: here,
  testMatch: "visual.spec.ts",
  timeout: 60_000,
  expect: {
    toHaveScreenshot: { maxDiffPixelRatio: 0.01, animations: "disabled" },
  },
  webServer: {
    command: "npx vite --config review/vite.config.ts --host 127.0.0.1 --strictPort",
    cwd: dirname(here),
    url: "http://127.0.0.1:5177",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
  use: {
    baseURL: "http://127.0.0.1:5177",
    viewport: { width: 1100, height: 800 },
  },
});
