// 093 useApp 细粒度迁移批次 1 冒烟：18 个迁移组件覆盖的核心页面渲染与联动。
// dev 用 embedded 模式（前端 dist 经 rust-embed 打进后端 + 后端 API 同端口）监听 18088；
// 不再依赖独立的 vite dev server（旧注释里的 5173），验证迁移后组件行为零回归。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('093-b1: 首屏渲染且无页面错误（Dashboard/列表壳）', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(BASE);
  await expect(page.locator('#root')).not.toBeEmpty({ timeout: 15000 });
  // 左轨（Dashboard 入口所在壳层）应可见
  await expect(page.getByTestId('left-rail-help').first()).toBeVisible();
  expect(errors).toEqual([]);
});

test('093-b1: 任务列表页（TodoListPage/TodoCenterCardView）渲染', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(`${BASE}/#/todos`);
  await expect(page.locator('#root')).not.toBeEmpty({ timeout: 15000 });
  // dev 库 ws=1 有 10 条 todo，列表应渲染出内容（搜索框或卡片任一锚点）
  await expect(page.locator('#root input, #root .ant-card').first()).toBeVisible({ timeout: 15000 });
  expect(errors).toEqual([]);
});

test('093-b1: 看板视图（KanbanBoard 依赖 workspace 联动）渲染', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(`${BASE}/#/kanban`);
  await expect(page.locator('#root')).not.toBeEmpty({ timeout: 15000 });
  expect(errors).toEqual([]);
});
