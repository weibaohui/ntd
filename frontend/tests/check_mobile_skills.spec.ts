import { test, expect } from '@playwright/test';

// 验证两个技能相关的移动端适配（375 视口，触发 useIsMobile 阈值 768）：
// 1. 设置→模板管理→Skill 模板 表格窄屏启用横向滚动（scroll={{ x: 'max-content' }}）。
// 2. 技能市场「全部技能」的来源筛选下拉窄屏改成单列（一行一个来源）、宽度贴近视口。
const BASE = 'http://localhost:18088';

test.use({ viewport: { width: 375, height: 812 } });

test('Skill 模板表格：窄屏启用横向滚动', async ({ page }) => {
  test.setTimeout(60000);
  // 直达模板管理 tab，再切到「Skill 模板」子 tab。
  await page.goto(`${BASE}/#/settings?tab=templates`);
  await page.waitForTimeout(1200);
  await page.locator('.ant-tabs-tab').filter({ hasText: 'Skill 模板' }).click();

  const tab = page.locator('.skill-templates-tab');
  await expect(tab).toBeVisible({ timeout: 10000 });
  const table = tab.locator('.ant-table').first();
  await expect(table).toBeVisible({ timeout: 10000 });

  // 修复前：6 列被压进容器，scrollWidth ≈ clientWidth；
  // 修复后：scroll={{ x: 'max-content' }} 列宽撑开，超出可横滑。
  const info = await table.evaluate((el) => {
    const scroller = el.querySelector('.ant-table-body') || el.querySelector('.ant-table-content') || el;
    return { sw: scroller.scrollWidth, cw: scroller.clientWidth };
  });
  console.log('[skill-table]', info);
  expect(info.sw, '窄屏 Skill 表格应可横向滚动').toBeGreaterThan(info.cw);
});

test('技能来源筛选下拉：窄屏单列一行一个来源', async ({ page }) => {
  test.setTimeout(60000);
  await page.goto(`${BASE}/#/skills`);
  await page.waitForTimeout(1200);
  // SkillsPanel 默认「总览」，切到「技能市场」才渲染 SkillMarketplace。
  await page.locator('.ant-segmented-item', { hasText: '技能市场' }).click();
  await page.waitForTimeout(800);
  // 切到「全部技能」视图，才会出现顶部来源筛选下拉。
  await page.getByRole('button', { name: '全部技能' }).click();
  await page.waitForTimeout(500);
  // 点开来源筛选下拉（默认文案「全部来源」）。
  await page.getByRole('button', { name: /全部来源/ }).click();
  const popup = page.locator('.ant-dropdown').last();
  await expect(popup).toBeVisible({ timeout: 5000 });

  // 取弹层内 grid 容器的计算样式：移动端应为单列（一行一个来源）。
  // （宽度由 calc(100vw-32px) 控制，下拉是 portal 渲染、getBoundingClientRect 不稳，
  //   故宽度以截图肉眼核验，这里只断言列数——即用户要的「一行一个」。）
  const cols = await popup.evaluate((root) => {
    for (const d of Array.from(root.querySelectorAll<HTMLElement>('div'))) {
      if (getComputedStyle(d).display === 'grid') {
        return getComputedStyle(d).gridTemplateColumns.split(' ').filter(Boolean).length;
      }
    }
    return -1;
  });
  console.log('[source-dropdown] cols =', cols);
  // gridTemplateColumns: 移动端 '1fr' → 1 列；桌面端计算值 '1fr 1fr' → 2 列。
  expect(cols, '来源筛选项应为单列（一行一个）').toBe(1);
  // 截图留档（目录已 gitignore），便于 PR 附图核验移动端单列 + 贴近视口宽度的效果。
  await page.screenshot({ path: 'tests/__screenshots__/mobile-skills-source-dropdown.png' });
});
