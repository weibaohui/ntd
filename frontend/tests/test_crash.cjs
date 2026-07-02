const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];

  page.on('console', msg => {
    if (msg.type() === 'error') errors.push('[console.error] ' + msg.text());
  });
  page.on('pageerror', err => errors.push('[pageerror] ' + err.message));

  await page.goto('http://localhost:18088/', { waitUntil: 'networkidle', timeout: 20000 });
  await page.waitForTimeout(3000);

  console.log('Page loaded. Title:', await page.title());

  // Find kanban cards
  const kanbanCards = await page.locator('.kanban-card').all();
  console.log('Kanban cards found:', kanbanCards.length);

  if (kanbanCards.length > 0) {
    console.log('Clicking first kanban card...');
    await kanbanCards[0].click();
    await page.waitForTimeout(3000);
    console.log('URL after click:', page.url());
    console.log('Errors after click:', JSON.stringify(errors, null, 2));
  } else {
    console.log('No kanban cards found, checking for any list items...');
    const items = await page.locator('.ant-list-item').all();
    console.log('List items:', items.length);
    if (items.length > 0) {
      await items[0].click();
      await page.waitForTimeout(2000);
      console.log('Errors:', JSON.stringify(errors));
    }
  }

  await page.screenshot({ path: '/tmp/bug_screen.png', fullPage: true });
  console.log('Screenshot saved to /tmp/bug_screen.png');

  if (errors.length > 0) {
    console.log('\n=== ERRORS ===');
    errors.forEach(e => console.log(e));
  } else {
    console.log('\nNo errors found!');
  }

  await browser.close();
})().catch(e => {
  console.error('Fatal error:', e.message);
  process.exit(1);
});
