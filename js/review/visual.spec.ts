import { expect, test } from "@playwright/test";
import { loadCasesFromDisk } from "./catalog/fromDisk.js";

const cases = await loadCasesFromDisk();

test("gallery figures match snapshots", async ({ page }) => {
  test.setTimeout(180_000);
  await page.goto("/");
  await expect(page.locator("[data-ready='1']")).toHaveCount(cases.length, {
    timeout: 120_000,
  });
  await expect(page.locator("[data-ready='error']")).toHaveCount(0);
  for (const c of cases) {
    const fig = page.locator(`[data-case="${c.id}"] .review-figure`);
    await fig.scrollIntoViewIfNeeded();
    await expect(fig).toHaveScreenshot(`${c.id}.png`);
  }
});
