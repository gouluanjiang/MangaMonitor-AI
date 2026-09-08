import { expect, test, type Page } from '@playwright/test';

// Each test gets a fresh browser context. Do not clear storage on page load:
// the close/reopen/reload test must exercise the app's own durable demo state.
test.use({ storageState: { cookies: [], origins: [] } });

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('demo-label')).toContainText('交互样例');
  await expect(page.getByTestId('demo-label')).toContainText('模拟数据');
});

async function selectPair(page: Page) {
  await page.getByTestId('select-rain').check();
  await page.getByTestId('select-flight').check();
  await page.getByTestId('batch-download').click();
  const dialog = page.getByTestId('confirm-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText('雨停之前');
  await expect(dialog).toContainText('白昼航线');
}

async function confirmPair(page: Page) {
  // One confirmation authorizes both selected demo tasks.
  await page.getByTestId('confirm-download').click();
  await expect(page.getByTestId('confirm-dialog')).toBeHidden();
  await page.getByTestId('nav-queue').click();
  await expect(page.getByTestId('queue-page')).toBeVisible();
  await expectPairOnce(page);
}

async function expectPairOnce(page: Page) {
  // Keep the three existing demo tasks as well as the two newly approved works.
  for (const workId of ['sea', 'moon', 'train', 'rain', 'flight']) {
    await expect(page.getByTestId(`task-${workId}`)).toHaveCount(1);
  }
  await expect(page.locator('[data-testid^="task-"]')).toHaveCount(5);
}

async function expectNoHorizontalOverflow(page: Page) {
  await expect.poll(async () => page.evaluate(() => {
    const contentWidth = Math.max(
      document.documentElement.scrollWidth,
      document.body.scrollWidth,
    );
    return contentWidth - document.documentElement.clientWidth;
  })).toBeLessThanOrEqual(1);
}

test('library search survives a visit to the independent detail page', async ({ page }) => {
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(8);
  await page.getByTestId('search-input').fill('雨');
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(1);
  await expect(page.getByTestId('card-rain')).toContainText('雨停之前');
  await page.getByTestId('open-rain').click();
  await expect(page.getByTestId('detail-page')).toBeVisible();
  await expect(page.getByTestId('detail-page')).toContainText('雨停之前');
  await page.getByTestId('back-library').click();
  await expect(page.getByTestId('search-input')).toHaveValue('雨');
  await expect(page.getByTestId('card-rain')).toBeVisible();
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(1);
  await page.getByTestId('search-input').fill('');
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(8);
});

test('one batch confirmation queues two works and prevents duplicate selection', async ({ page }) => {
  await selectPair(page);
  await confirmPair(page);
  await page.getByTestId('nav-library').click();
  await expect(page.getByTestId('select-rain')).toBeDisabled();
  await expect(page.getByTestId('select-flight')).toBeDisabled();
  await page.getByTestId('nav-queue').click();
  await expectPairOnce(page);
});

test('an unresolved review work has no batch-download selection', async ({ page }) => {
  const review = page.getByTestId('card-echo');
  await expect(review).toContainText('星光回声');
  await expect(review.getByRole('checkbox')).toHaveCount(0);
  await expect(page.getByTestId('select-echo')).toHaveCount(0);
  await page.getByTestId('nav-queue').click();
  await expect(page.getByTestId('task-echo')).toHaveCount(0);
});

test('safe close and reopen retain tasks and an explicitly paused queue', async ({ page }) => {
  await selectPair(page);
  await confirmPair(page);
  await page.getByTestId('pause-queue').click();
  await expect(page.getByTestId('pause-queue')).toContainText('继续队列');
  await page.getByTestId('demo-offline').click();
  await page.getByTestId('demo-close').click();
  await expect(page.getByTestId('demo-reopen')).toBeVisible();
  await page.getByTestId('demo-reopen').click();
  await page.getByTestId('nav-queue').click();
  await expectPairOnce(page);
  await expect(page.getByTestId('pause-queue')).toContainText('继续队列');

  // Reopening the overlay alone could pass with in-memory state. A real reload
  // also verifies that queued tasks and the user's pause choice were persisted.
  await page.reload();
  await page.getByTestId('nav-queue').click();
  await expectPairOnce(page);
  await expect(page.getByTestId('pause-queue')).toContainText('继续队列');
});

test.describe('390px workbench layout', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('library, detail, confirmation, queue and settings fit the viewport', async ({ page }) => {
    await expect(page.getByTestId('card-rain')).toBeVisible();
    await expectNoHorizontalOverflow(page);
    await page.getByTestId('open-rain').click();
    await expect(page.getByTestId('detail-page')).toBeVisible();
    await expectNoHorizontalOverflow(page);
    await page.getByTestId('back-library').click();
    await selectPair(page);
    await expectNoHorizontalOverflow(page);
    await confirmPair(page);
    await expectNoHorizontalOverflow(page);
    await page.getByTestId('nav-settings').click();
    await expectNoHorizontalOverflow(page);
  });
});
