// 107：智能助手（Bots）面板 UI 回归（功能清单 F14）。
// 覆盖：
// 1. Bot 列表表格渲染（名称/类型/状态/操作列）；
// 2. 「绑定智能助手」入口存在。
//
// 运行前提：make dev 已起（18088）。Bots 数据依赖后端（dev 库已有测试 Bot），
// 若列表为空则断言空态而非失败（数据依赖容忍）。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('F14 智能助手：列表与入口', async ({ page }) => {
  await page.goto(`${BASE}/#/bots`);
  await expect(page.locator('main')).toContainText('智能助手', { timeout: 10000 });

  // —— 1. 表格渲染（列头） ——
  const headers = await page.locator('.ant-table-thead th').allInnerTexts();
  const headerText = headers.join('|');
  expect(headerText).toContain('名称');
  expect(headerText).toContain('类型');

  // —— 2. 数据行或空态 ——
  const rows = await page.locator('tbody tr.ant-table-row').count();
  if (rows > 0) {
    // 有数据：状态列与操作列存在
    expect(headerText).toContain('状态');
    expect(headerText).toContain('操作');
  } else {
    // 无数据：允许空态（数据依赖），但不允许白屏
    await expect(page.locator('.ant-empty, tbody')).toBeVisible();
  }

  // —— 3. 绑定入口 ——
  const bindBtn = page.getByRole('button', { name: /绑定智能助手/ }).first();
  await expect(bindBtn).toBeVisible();
});
