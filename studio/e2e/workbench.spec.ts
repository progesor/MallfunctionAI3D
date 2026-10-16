import { expect, test } from '@playwright/test';

async function prepare(page: import('@playwright/test').Page) {
  await page.goto('/');
  await expect(page.locator('.viewport-frame')).toBeVisible();
  // Browser-preview mode has no Tauri IPC; this informational banner must not
  // overlap the viewport buttons during the layout-only regression tests.
  const dismiss = page.getByRole('button', { name: 'Hata bildirimini kapat' });
  if (await dismiss.isVisible()) await dismiss.click();
}

for (const dimensions of [
  { width: 1480, height: 920 },
  { width: 1090, height: 710 },
  { width: 1920, height: 1080 },
]) {
  test(`viewport really expands and restores at ${dimensions.width}x${dimensions.height}`, async ({ page }) => {
    await page.setViewportSize(dimensions);
    await prepare(page);
    const frame = page.locator('.viewport-frame');
    const before = await frame.boundingBox();
    expect(before).not.toBeNull();
    await page.getByRole('button', { name: 'Görünümü büyüt' }).click();
    await expect(page.locator('.app-shell')).toHaveClass(/focus-mode/);
    await expect(page.locator('.tools-column')).toBeHidden();
    await expect(page.locator('.inspector-column')).toBeHidden();
    await expect(page.locator('.activity-strip')).toBeHidden();
    const after = await frame.boundingBox();
    expect(after).not.toBeNull();
    expect(after!.width).toBeGreaterThan(before!.width + 80);
    expect(after!.height).toBeGreaterThan(before!.height + 35);
    expect(after!.width).toBeGreaterThanOrEqual(dimensions.width - 24);
    await page.keyboard.press('Escape');
    await expect(page.locator('.app-shell')).not.toHaveClass(/focus-mode/);
    const restored = await frame.boundingBox();
    expect(Math.abs(restored!.width - before!.width)).toBeLessThan(3);
    expect(Math.abs(restored!.height - before!.height)).toBeLessThan(3);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(dimensions.width);
  });
}

test('side panels resize the viewer and primary Generate stays visible', async ({ page }) => {
  await page.setViewportSize({ width: 1480, height: 920 });
  await prepare(page);
  const frame = page.locator('.viewport-frame');
  const original = (await frame.boundingBox())!;
  const generate = page.getByRole('button', { name: /3D Model Üret/ });
  await expect(generate).toBeVisible();
  await page.getByRole('button', { name: 'Üretim panelini gizle' }).click();
  await expect(page.locator('.tools-column')).toBeHidden();
  const widerLeft = (await frame.boundingBox())!;
  expect(widerLeft.width).toBeGreaterThan(original.width + 150);
  await page.getByRole('button', { name: 'Üretim panelini göster' }).click();
  await expect(generate).toBeVisible();
  await page.getByRole('button', { name: 'Asset Inspector panelini gizle' }).click();
  await expect(page.locator('.inspector-column')).toBeHidden();
  const widerRight = (await frame.boundingBox())!;
  expect(widerRight.width).toBeGreaterThan(original.width + 150);
  await page.getByRole('button', { name: 'Asset Inspector panelini göster' }).click();
  await expect(page.locator('.inspector-column')).toBeVisible();
  const restored = (await frame.boundingBox())!;
  expect(Math.abs(restored.width - original.width)).toBeLessThan(3);
});

test('all production modes fit without hiding the main action', async ({ page }) => {
  await page.setViewportSize({ width: 1090, height: 710 });
  await prepare(page);
  await expect(page.getByRole('button', { name: /3D Model Üret/ })).toBeVisible();
  await page.getByRole('button', { name: /Özel ayarlar/ }).click();
  const controls = page.locator('.tools-column .panel-content');
  await controls.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await expect(page.getByRole('button', { name: /3D Model Üret/ })).toBeVisible();
  await expect(page.locator('.production-action-bar')).toBeInViewport();
  await expect(page.locator('.viewport-frame')).toBeInViewport();
});
