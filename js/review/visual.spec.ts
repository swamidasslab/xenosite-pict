import { expect, test } from "@playwright/test";
import { loadCasesFromDisk } from "./catalog/fromDisk.js";

const cases = await loadCasesFromDisk();

test("gallery figures match snapshots", async ({ page }) => {
  test.setTimeout(180_000);
  await page.goto("/?expand=all");
  await expect(page.locator(".group")).toHaveCount(
    new Set(cases.map((c) => c.group)).size,
  );
  for (const c of cases) {
    const card = page.locator(`[data-case="${c.id}"]`);
    await card.scrollIntoViewIfNeeded();
    await expect(card.locator("[data-ready='1']")).toHaveCount(1, {
      timeout: 30_000,
    });
    await expect(card.locator("[data-ready='error']")).toHaveCount(0);
    await expect(card.locator(".review-figure")).toHaveScreenshot(`${c.id}.png`);
  }
});
