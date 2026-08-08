// 093 首屏 bundle 瘦身验证（docs/design/093-首屏bundle瘦身-设计.md §5）。
//
// 验证点：
// 1. 首页 HTML 的 modulepreload 不再包含 vendor-monaco / vendor-md-editor / vendor-flow；
// 2. 首屏可正常渲染（React 应用挂载成功）；
// 3. 含 markdown 的区域（LazyXMarkdown）异步加载后真正渲染出 HTML；
// 4. 帮助页（懒加载 MermaidDiagram + LazyXMarkdown 叠加路径）渲染正常。
//
// 依赖：make dev 已起 18088（embedded 模式直接服务 dist 产物，与生产同路径）。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('093: 首屏 HTML 不再 preload monaco/md-editor/flow 三个重型 chunk', async ({ request }) => {
  // 直接取 HTML 文本断言 preload 列表——这是本专项的核心验收，比页面行为更直接
  const html = await (await request.get(`${BASE}/`)).text();
  expect(html).not.toMatch(/modulepreload[^>]*vendor-monaco/);
  expect(html).not.toMatch(/modulepreload[^>]*vendor-md-editor/);
  expect(html).not.toMatch(/modulepreload[^>]*vendor-flow/);
  // 但 antd 等首屏必需 chunk 必须仍在，防止「一刀切全不 preload」的误伤
  expect(html).toMatch(/modulepreload[^>]*vendor-antd/);
});

test('093: 首屏正常渲染，monaco/md-editor/flow chunk 不在首批网络请求中', async ({ page }) => {
  const firstScreenRequests: string[] = [];
  page.on('request', (req) => firstScreenRequests.push(req.url()));

  await page.goto(BASE);
  // React 根节点挂出内容即视为首屏成功
  await expect(page.locator('#root')).not.toBeEmpty({ timeout: 15000 });

  // 首屏阶段（goto 完成即断言）不应触发重型 chunk 下载；
  // 注意不能等太久——用户后续操作（如打开抽屉）触发加载是合法行为
  const heavy = firstScreenRequests.filter((u) =>
    /vendor-monaco|vendor-md-editor|vendor-flow/.test(u),
  );
  expect(heavy).toEqual([]);
});

test('093: 帮助页 markdown 经懒加载后正常渲染', async ({ page }) => {
  await page.goto(BASE);
  await expect(page.locator('#root')).not.toBeEmpty({ timeout: 15000 });

  // 帮助页不是 hash 路由，而是 LeftRail 的「帮助」按钮触发的抽屉（App.tsx: HelpPage 组件）；
  // 通过 data-testid 点击进入，这是 093 懒加载改造的重点回归路径（LazyXMarkdown + 懒加载 MermaidDiagram 叠加）
  await page.getByTestId('left-rail-help').first().click();
  // antd Drawer/Modal 内容 portal 到 body 而非 #root 内，选择器不能带 #root 前缀。
  // LazyXMarkdown 异步 chunk 就绪后，帮助正文应渲染出真实 HTML（而非停留在纯文本兜底）
  await expect(
    page.locator('.ant-modal h1, .ant-modal h2, .ant-modal p, .ant-drawer h1, .ant-drawer h2, .ant-drawer p').first(),
  ).toBeVisible({ timeout: 15000 });
});
