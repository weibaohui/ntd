import { test, expect, type Page, type Route } from '@playwright/test';

// 执行器管理面板的「一键安装」入口。
// 由于本机实际安装情况不可控，用 page.route 拦截 /api/v1/executors 与 detect 接口，
// 构造一个「已安装」和一个「未安装」执行器，验证安装按钮仅在未安装行出现。

const BASE = 'http://localhost:18088';

/** 构造执行器列表响应体。 */
function executorsPayload() {
  return {
    code: 0,
    data: [
      {
        id: 1,
        name: 'claudecode',
        path: 'claude',
        enabled: true,
        display_name: 'Claude Code',
        session_dir: '~/.claude',
        is_default: true,
        default_model: null,
        supports_models: false,
        created_at: null,
        updated_at: null,
      },
      {
        id: 2,
        name: 'codex',
        path: 'codex',
        enabled: false,
        display_name: 'Codex',
        session_dir: '~/.codex',
        is_default: false,
        default_model: null,
        supports_models: false,
        created_at: null,
        updated_at: null,
      },
    ],
    message: '',
  };
}

/** 构造 detect 响应体。 */
function detectPayload(found: boolean, path: string | null) {
  return {
    code: 0,
    data: { binary_found: found, path_resolved: path },
    message: '',
  };
}

/**
 * 注册路由拦截：
 * - executors 列表返回 2 条
 * - claudecode detect 成功
 * - codex detect 失败
 */
async function interceptExecutors(page: Page) {
  // 前端代码写 /api/executors，axios 拦截器重写为 /api/v1/executors；
  // page.route 在浏览器网络层拦截，两个路径都兜住以确保命中。
  const handlers = {
    executors: (route: Route) =>
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify(executorsPayload()),
      }),
    claudeDetect: (route: Route) =>
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify(detectPayload(true, '/usr/local/bin/claude')),
      }),
    codexDetect: (route: Route) =>
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify(detectPayload(false, null)),
      }),
  };
  await page.route('**/api/executors', handlers.executors);
  await page.route('**/api/v1/executors', handlers.executors);
  await page.route('**/api/executors/claudecode/detect', handlers.claudeDetect);
  await page.route('**/api/v1/executors/claudecode/detect', handlers.claudeDetect);
  await page.route('**/api/executors/codex/detect', handlers.codexDetect);
  await page.route('**/api/v1/executors/codex/detect', handlers.codexDetect);
}

/** 进入执行器管理页并等待渲染。 */
async function openExecutorsPanel(page: Page) {
  await page.goto(`${BASE}/#executors`);
  // 等待表格出现；首次加载需要拉取执行器列表并渲染
  await page.waitForSelector('.ant-table-row');
}

test('已安装执行器行不显示安装按钮，未安装行显示安装按钮', async ({ page }) => {
  await interceptExecutors(page);
  await openExecutorsPanel(page);

  // 点击批量检测，触发 detect 并刷新状态
  // ant-design 中文按钮 accessible name 可能带空格，用正则兼容
  await page.getByRole('button', { name: /批\s*量\s*检\s*测/ }).click();
  await page.waitForTimeout(1200);

  // 找到 Codex 行，应出现「安装」按钮
  const codexRow = page.locator('.ant-table-row').filter({ hasText: 'Codex' });
  // 安装按钮在空间紧张时不显示文字，只保留 download 图标；按图标 aria-label 定位
  await expect(codexRow.getByRole('button', { name: /download/i })).toBeVisible();

  // Claude Code 行不应出现「安装」按钮
  const claudeRow = page.locator('.ant-table-row').filter({ hasText: 'Claude Code' });
  await expect(claudeRow.getByRole('button', { name: /download/i })).toHaveCount(0);
});

test('点击安装按钮打开 Drawer 并展示执行器名', async ({ page }) => {
  await interceptExecutors(page);
  await openExecutorsPanel(page);

  await page.getByRole('button', { name: /批量检测/ }).click();
  await page.waitForTimeout(800);

  const codexRow = page.locator('.ant-table-row').filter({ hasText: 'Codex' });
  await codexRow.getByRole('button', { name: /download/i }).click();

  // Drawer 标题应包含 Codex
  await expect(page.locator('.ant-drawer-title')).toContainText('安装 Codex');
  // prompt 编辑区应存在（限定在 Drawer 体内，避免命中 antd 内部隐藏 textarea）
  await expect(page.locator('.ant-drawer-body textarea').first()).toBeVisible();
});

test('点击取消关闭安装 Drawer', async ({ page }) => {
  await interceptExecutors(page);
  await openExecutorsPanel(page);

  await page.getByRole('button', { name: /批量检测/ }).click();
  await page.waitForTimeout(800);

  const codexRow = page.locator('.ant-table-row').filter({ hasText: 'Codex' });
  await codexRow.getByRole('button', { name: /download/i }).click();
  await expect(page.locator('.ant-drawer')).toBeVisible();

  await page.getByRole('button', { name: /取\s*消/ }).click();
  await expect(page.locator('.ant-drawer')).toHaveCount(0);
});
