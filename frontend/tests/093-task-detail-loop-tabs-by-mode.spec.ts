// 093-任务详情环路Tab按执行方式显隐 Playwright 验证（设计 093）。
// 验证点：
// 1. 委派任务（execution_mode=delegate）详情只展示「概览」「讨论」两 Tab，无「执行环路」「执行历史」；
// 2. 工艺环路任务（execution_mode=loop）详情仍展示「执行环路」「执行历史」两 Tab；
// 3. 委派任务 URL 残留 ?tab=dag 时，不出现内容区空白——自动回退到默认 Tab（讨论）。
//
// 运行前提：make dev 已起（18088）。
// 数据依赖（dev 库 ~/.ntd/data.dev.db，均属 workspace 3）：
//   - 委派任务 fixture id=52（execution_mode=delegate, loop_id=NULL），为本验证直接插入；
//   - 工艺环路任务 id=15（execution_mode=loop, loop_id=21），dev 库既有数据。
// 这与 060 spec 复用 dev 库既有任务（如 id=39）的做法一致。

import { test, expect, type Page } from '@playwright/test';

const BASE = 'http://localhost:18088';
// 委派任务 fixture：execution_mode=delegate，无 loop_id → 不应出现环路相关 Tab。
const DELEGATE_TASK_ID = 52;
// 工艺环路任务：execution_mode=loop，loop_id=21 → 应出现环路相关 Tab。
const LOOP_TASK_ID = 15;

// 等待任务详情 Tab 栏渲染完成，返回各 Tab 的可见文本（便于按文案断言）。
// 用 innerText 逐个收集，避免 locator.allTextItems 受 Badge/图标节点干扰。
async function getTabTexts(page: Page): Promise<string[]> {
  await page.waitForSelector('.ant-tabs-tab', { timeout: 15000 });
  const tabs = page.locator('.ant-tabs-tab');
  const count = await tabs.count();
  const texts: string[] = [];
  for (let i = 0; i < count; i++) {
    texts.push((await tabs.nth(i).innerText()) ?? '');
  }
  return texts;
}

test('委派任务详情：只展示概览与讨论，无环路相关 Tab', async ({ page }) => {
  await page.goto(`${BASE}/#/tasks/${DELEGATE_TASK_ID}`);
  const texts = await getTabTexts(page);

  // 委派任务应恰好 2 个 Tab：概览、讨论。
  expect(texts.length).toBe(2);
  const joined = texts.join('|');
  expect(joined).toContain('概览');
  expect(joined).toContain('讨论');
  // 关键断言：环路相关 Tab 不应出现。
  expect(texts.some((t) => t.includes('执行环路'))).toBe(false);
  expect(texts.some((t) => t.includes('执行历史'))).toBe(false);
});

test('工艺环路任务详情：仍展示执行环路/执行历史 Tab', async ({ page }) => {
  await page.goto(`${BASE}/#/tasks/${LOOP_TASK_ID}`);
  const texts = await getTabTexts(page);

  // 环路任务应保留环路相关 Tab（label 形如「执行环路 (N)」「执行历史」）。
  expect(texts.some((t) => t.includes('执行环路'))).toBe(true);
  expect(texts.some((t) => t.includes('执行历史'))).toBe(true);
});

test('委派任务 URL 残留 ?tab=dag：回退默认 Tab，内容区不空白', async ({ page }) => {
  // resolvedTab 会把 ?tab=dag 解析为 'dag'（TAB_KEYS 白名单含该项），但委派任务已隐藏该 Tab。
  // 若无 activeKey 兜底，Tabs 会落到无选中态、内容区空白；此处验证兜底回退到「讨论」。
  await page.goto(`${BASE}/#/tasks/${DELEGATE_TASK_ID}?tab=dag`);
  await page.waitForSelector('.ant-tabs-tab', { timeout: 15000 });

  // 激活的 Tab 应回退为「讨论」（委派任务默认 Tab）。
  const activeTab = page.locator('.ant-tabs-tab-active');
  await expect(activeTab).toBeVisible();
  const activeText = (await activeTab.innerText()) ?? '';
  expect(activeText).toContain('讨论');

  // 内容区不空白：讨论 Tab forceRender 挂载，发送按钮可见（DiscussionComposer 已渲染）。
  await expect(page.getByRole('button', { name: '发送' })).toBeVisible({ timeout: 10000 });
});
