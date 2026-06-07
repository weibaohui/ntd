const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];

  page.on('console', msg => {
    if (msg.type() === 'error') errors.push('[E] ' + msg.text());
  });
  page.on('pageerror', err => errors.push('[PE] ' + err.message));

  await page.goto('http://localhost:18088/', { waitUntil: 'networkidle', timeout: 20000 });
  await page.waitForTimeout(3000);

  // Get the todo items in the list
  const todoItems = await page.locator('.todo-item').all();
  console.log('Todo items found:', todoItems.length);

  if (todoItems.length > 0) {
    // Get text of first item
    const firstText = await todoItems[0].innerText();
    console.log('First todo text:', firstText.substring(0, 80));

    // Click the todo item
    console.log('Clicking first todo item...');
    await todoItems[0].click();
    await page.waitForTimeout(2000);

    console.log('URL after click:', page.url());
    console.log('Errors after click:', JSON.stringify(errors));

    // Check if detail panel appeared
    const detailPanel = await page.locator('.detail-panel, .detail-panel-wide').count();
    console.log('Detail panel visible:', detailPanel > 0);

    // Take screenshot
    await page.screenshot({ path: '/tmp/after_todo_click.png', fullPage: true });
    console.log('Screenshot saved');
  } else {
    console.log('No todo items found with .todo-item class');

    // Try clicking anywhere in the todo list area
    const bodyText = await page.locator('body').innerText();
    console.log('Body text sample:', bodyText.substring(0, 300));
  }

  // Check URL params
  const url = page.url();
  console.log('Final URL:', url);

  await browser.close();
})().catch(e => {
  console.error('Fatal:', e.message);
  process.exit(1);
});
