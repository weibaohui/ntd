import { test, expect } from '@playwright/test';

// 模板管理面板的「Git 检测 + 一键安装」入口。
// git 是 bundled 资源同步的硬前置依赖。本机装了 git，正常请求 git_available=true、不显示缺失横幅；
// 因此用 page.route 拦截 /api/bundled/status 强制返回 git_available:false，才能验证「缺失态」UI。
// 成功信封为 { code:0, data, message }（见 utils/database/client.ts 的 unwrap）。

const BASE = 'http://localhost:18088';

/** 构造一份 status 响应体，仅 git_available 可变，其余给稳定默认值。 */
function statusPayload(gitAvailable: boolean) {
  return {
    code: 0,
    data: {
      remote_url: 'https://example.com/test.git',
      branch: 'main',
      local_path: '/tmp/test-bundled',
      sync_strategy: 'overwrite',
      auto_sync_enabled: false,
      local_exists: true,
      local_commit: 'abc12345',
      remote_commit: 'abc12345',
      needs_update: false,
      last_sync_at: null,
      subdir: 'all',
      subdir_exists: true,
      subdir_file_count: 3,
      git_available: gitAvailable,
    },
    message: '',
  };
}

/** 拦截 status 接口 + 进入模板管理 tab 的公共前置。 */
async function openTemplatesPanel(page: import('@playwright/test').Page, gitAvailable: boolean) {
  // 路由必须在 goto 之前注册，才能命中面板挂载时的首次 status 请求
  await page.route('**/api/bundled/status**', (route) =>
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify(statusPayload(gitAvailable)),
    })
  );
  await page.goto(`${BASE}/#settings`);
  await page.getByRole('tab', { name: /模板管理/ }).click();
  // 等待 useEffect 拉取 status 并完成渲染
  await page.waitForTimeout(800);
}

test('git 缺失时模板管理页顶部展示告警 + 一键安装按钮', async ({ page }) => {
  await openTemplatesPanel(page, false);

  // 顶部 Alert 的 message 文案应可见
  await expect(page.getByText('未检测到 Git')).toBeVisible();
  // ActionButton 渲染出的「安装 Git」按钮应存在，即一键安装入口已挂上
  await expect(page.getByRole('button', { name: /安装 Git/ })).toBeVisible();
});

test('git 可用时不展示缺失告警', async ({ page }) => {
  await openTemplatesPanel(page, true);

  // git_available=true 时缺失告警不应渲染
  await expect(page.getByText('未检测到 Git')).toHaveCount(0);
  // 也不应出现安装按钮（只有缺失态才挂）
  await expect(page.getByRole('button', { name: /安装 Git/ })).toHaveCount(0);
});

test('点击「同步状态」弹窗里展示 Git 运行环境行（缺失态带安装按钮）', async ({ page }) => {
  await openTemplatesPanel(page, false);

  // 打开同步状态弹窗
  await page.getByRole('button', { name: /同步状态/ }).click();
  await page.waitForTimeout(500);

  // 弹窗内的 Descriptions 应包含「Git 运行环境」这一行
  await expect(page.getByText('Git 运行环境')).toBeVisible();
  // 缺失态下弹窗内同样有「安装 Git」按钮（与顶部横幅共两个）
  await expect(page.getByRole('button', { name: /安装 Git/ })).toHaveCount(2);
});
