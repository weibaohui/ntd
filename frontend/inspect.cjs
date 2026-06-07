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

  // Get page content structure
  const info = await page.evaluate(() => {
    const root = document.getElementById('root');
    // Find all elements that look like clickable todos
    const cards = document.querySelectorAll('.ant-list-item, .ant-card, .kanban-card, .todo-card, [draggable]');
    const cardInfo = Array.from(cards).slice(0, 5).map(e => ({
      tag: e.tagName,
      classes: e.className,
      text: e.innerText ? e.innerText.substring(0, 50) : ''
    }));

    // Check what's visible in the main content
    const mainContent = document.querySelector('.ant-layout-content');
    const contentText = mainContent ? mainContent.innerText.substring(0, 200) : 'no content';

    return { cardCount: cards.length, cardInfo, contentText };
  });

  console.log('Cards found:', info.cardCount);
  console.log('Card info:', JSON.stringify(info.cardInfo, null, 2));
  console.log('Content text:', info.contentText);

  // Try to click on the first card
  if (info.cardCount > 0) {
    console.log('Clicking first card...');
    await page.evaluate(() => {
      const card = document.querySelector('.ant-list-item, .ant-card, .kanban-card, .todo-card, [draggable]');
      if (card) card.click();
    });
    await page.waitForTimeout(2000);
    console.log('URL after click:', page.url());
    console.log('Errors:', errors);
  }

  // Also look for kanban button and click it
  const kanbanBtn = await page.locator('button[aria-label="看板"]').first();
  if (await kanbanBtn.isVisible()) {
    console.log('Clicking kanban button...');
    await kanbanBtn.click();
    await page.waitForTimeout(2000);
    console.log('URL after kanban click:', page.url());
    console.log('Errors:', errors);

    // Now try clicking a kanban card
    const kanbanCards = await page.locator('.kanban-card').all();
    console.log('Kanban cards after switch:', kanbanCards.length);
    if (kanbanCards.length > 0) {
      await kanbanCards[0].click();
      await page.waitForTimeout(2000);
      console.log('URL after kanban card click:', page.url());
      console.log('Errors after card click:', errors);
    }
  }

  await page.screenshot({ path: '/tmp/bug_screen.png', fullPage: true });
  console.log('Screenshot saved');

  await browser.close();
})().catch(e => {
  console.error('Fatal:', e.message);
  process.exit(1);
});
