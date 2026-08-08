// 093-任务详情环路Tab按执行方式显隐 Playwright 验证（设计 093）。
// 验证点：
// 1. 委派任务（execution_mode=delegate）详情只展示「概览」「讨论」两 Tab，无「执行环路」「执行历史」；
// 2. 工艺环路任务（execution_mode=loop）详情仍展示全部 4 个 Tab（设计 §4「4 个 Tab 齐全」）；
// 3. 委派任务 URL 残留 ?tab=dag 时，不出现内容区空白——自动回退到默认 Tab（讨论）。
//
// 运行前提：make dev 已起（18088）。
// 数据独立（前端规范 11-测试规范 §4「测试必须独立可运行，不依赖外部状态」）：
//   - 委派任务：beforeAll 向 dev 库插入 fixture，afterAll 按唯一标记清理，不依赖任何预置数据；
//   - 工艺环路任务：dev 库通常已有 loop 任务，动态取第一条；若无则跳过（loop 任务需工艺 install 产生，本 spec 不便造数）。

import { test, expect, type Page } from '@playwright/test';
import { execSync } from 'child_process';
import * as path from 'path';

const BASE = 'http://localhost:18088';
// dev 库路径（CLAUDE.md 开发环境约定），用 HOME 展开避免硬编码用户目录。
const DEV_DB = path.join(process.env.HOME ?? '', '.ntd', 'data.dev.db');
// 委派 fixture 所属工作空间：与 dev 库现有 loop 任务一致，便于在同一 ws 内校验。
const WS_ID = 3;
// 唯一标记：afterAll 按此前缀精确清理本 spec 插入的行，绝不动到既有数据。
const FIXTURE_TITLE = '093-pw-fixture-delegate';

let delegateTaskId = 0;
let loopTaskId = 0;

test.beforeAll(() => {
  // 插入委派任务 fixture：execution_mode=delegate，loop_id 默认 NULL → 不应出现环路相关 Tab。
  execSync(
    `sqlite3 "${DEV_DB}" "INSERT INTO tasks ` +
      `(title, description, status, workspace_id, execution_mode, assignee_kind, assignee_name, ` +
      `auto_continue, continue_rounds, created_at, updated_at) ` +
      `VALUES ('${FIXTURE_TITLE}', 'Playwright 验证委派任务', 'pending', ${WS_ID}, ` +
      `'delegate', 'executor', 'codex', 0, 0, datetime('now'), datetime('now'));"`,
  );
  // 读回自增 id，供用例导航。
  delegateTaskId = Number(
    execSync(
      `sqlite3 "${DEV_DB}" "SELECT id FROM tasks WHERE title='${FIXTURE_TITLE}' ORDER BY id DESC LIMIT 1;"`,
    )
      .toString()
      .trim(),
  );

  // 动态发现一条工艺环路任务（loop 且 loop_id 非空）；找不到则 loopTaskId 留 0，用例内 skip。
  const loopOut = execSync(
    `sqlite3 "${DEV_DB}" "SELECT id FROM tasks WHERE execution_mode='loop' AND loop_id IS NOT NULL ORDER BY id LIMIT 1;"`,
  )
    .toString()
    .trim();
  loopTaskId = loopOut ? Number(loopOut) : 0;
});

test.afterAll(() => {
  // 仅清理本 spec 插入的 fixture（按标题精确匹配），其余数据原样保留。
  execSync(`sqlite3 "${DEV_DB}" "DELETE FROM tasks WHERE title='${FIXTURE_TITLE}';"`);
});

// 等待任务详情 Tab 栏渲染完成，返回各 Tab 的可见文本。逐个 innerText 收集，避免 Badge/图标节点干扰。
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
  await page.goto(`${BASE}/#/tasks/${delegateTaskId}`);
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

test('工艺环路任务详情：仍展示全部 4 个 Tab', async ({ page }) => {
  // dev 库无 loop 任务时跳过：loop 任务需工艺 install 产生，本 spec 不便造数。
  test.skip(!loopTaskId, 'dev 库无工艺环路任务，跳过该用例');
  await page.goto(`${BASE}/#/tasks/${loopTaskId}`);
  const texts = await getTabTexts(page);

  // 设计 §4 测试计划：断言 4 个 Tab 齐全（概览/执行环路/执行历史/讨论）。
  expect(texts.length).toBe(4);
  const joined = texts.join('|');
  expect(joined).toContain('概览');
  expect(joined).toContain('讨论');
  expect(texts.some((t) => t.includes('执行环路'))).toBe(true);
  expect(texts.some((t) => t.includes('执行历史'))).toBe(true);
});

test('委派任务 URL 残留 ?tab=dag：回退默认 Tab，内容区不空白', async ({ page }) => {
  // resolvedTab 会把 ?tab=dag 解析为 'dag'（TAB_KEYS 白名单含该项），但委派任务已隐藏该 Tab。
  // 若无 activeKey 兜底，Tabs 会落到无选中态、内容区空白；此处验证兜底回退到「讨论」。
  await page.goto(`${BASE}/#/tasks/${delegateTaskId}?tab=dag`);
  await page.waitForSelector('.ant-tabs-tab', { timeout: 15000 });

  // 激活的 Tab 应回退为「讨论」（委派任务默认 Tab）。
  const activeTab = page.locator('.ant-tabs-tab-active');
  await expect(activeTab).toBeVisible();
  const activeText = (await activeTab.innerText()) ?? '';
  expect(activeText).toContain('讨论');

  // 内容区不空白：讨论 Tab forceRender 挂载，发送按钮可见（DiscussionComposer 已渲染）。
  await expect(page.getByRole('button', { name: '发送' })).toBeVisible({ timeout: 10000 });
});
