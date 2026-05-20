const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext();
  const page = await context.newPage();

  const errors = [];
  page.on('console', msg => {
    if (msg.type() === 'error') {
      errors.push(msg.text());
    }
  });
  page.on('pageerror', err => {
    errors.push(err.message);
  });

  try {
    await page.goto('http://localhost:18088', { timeout: 30000 });
    await page.waitForTimeout(3000);

    const title = await page.title();
    console.log('Page title:', title);

    const dashboardExists = await page.locator('.dashboard-card, [class*="dashboard"]').count();
    console.log('Dashboard elements found:', dashboardExists);

    const metricCards = await page.locator('text=今日执行, text=总执行, text=成功率').count();
    console.log('Metric cards found:', metricCards);

    if (errors.length > 0) {
      console.log('Console errors:', errors);
      process.exit(1);
    } else {
      console.log('No console errors detected!');
      console.log('Dashboard loaded successfully!');
    }
  } catch (err) {
    console.error('Test failed:', err.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
})();
