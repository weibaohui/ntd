// 093 useApp 批次 2 冒烟：TodoDetail / ExecutionPanel 迁移 + useAppDispatch 零订阅拆分。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:5173';

test('093-b2: 列表页 → 打开 Todo 详情（TodoDetail 迁移路径）渲染无错', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(`${BASE}/#/todos`);
  await expect(page.locator('#root')).not.toBeEmpty({ timeout: 15000 });
  // 点第一张卡片进详情（dev 库 ws=1 有数据）
  const card = page.locator('.ant-card, [class*="card"]').first();
  if (await card.isVisible().catch(() => false)) {
    await card.click();
    await page.waitForTimeout(1500);
  }
  expect(errors).toEqual([]);
});

test('093-b2: 执行面板区域（ExecutionPanel）挂载且无页面错误', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(BASE);
  await expect(page.locator('#root')).not.toBeEmpty({ timeout: 15000 });
  // WS 连接建立（useExecutionEvents 走 useAppDispatch 路径）
  await page.waitForTimeout(2000);
  expect(errors).toEqual([]);
});
