import { test, expect } from '@playwright/test';

/**
 * ADR-005 / H5：ProfilesPanel 直连 fetch 收敛为 providers Facade 后的冒烟验证。
 *
 * API Key 面板挂在「执行器」页的「API Key」tab 下。进入该 tab 会触发
 * ProfilesPanel 的加载流程：listProviders() + getSupportedExecutors() + 对每个
 * provider 并发 getProvider()。这些调用现已全部走 providers Facade（client.ts 的
 * axios 实例）。若 Facade 端点/方法/解包写错，面板会报错或空，本用例据此拦截回归。
 *
 * 注：provider 是全局服务端配置（非 workspace 维度），故不按 workspace 固定选择，
 * 与 verify_executor_tabs.spec.ts 的导航方式保持一致。
 */

test('API Key 面板经 providers Facade 加载正常', async ({ page }) => {
  // 采集页面级报错与 antd 错误 toast，加载完成后断言其未出现，
  // 用来兜住 Facade 调用失败被静默吞掉的情况。
  const pageErrors: string[] = [];
  page.on('pageerror', (e) => pageErrors.push(e.message));

  await page.goto('http://localhost:18088');
  await page.waitForLoadState('networkidle');

  // 进入「执行器」设置页。
  await page.click('[data-testid="left-rail-settings_executors"]');
  await page.waitForTimeout(800);

  // 切到「API Key」tab，挂载 ProfilesPanel 并触发加载流程。
  const apiKeyTab = page.locator('.ant-tabs-tab').filter({ hasText: 'API Key' });
  await expect(apiKeyTab).toBeVisible();
  await apiKeyTab.click();
  // 等待 listProviders + getSupportedExecutors + 逐个 getProvider 的请求收尾。
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(500);

  // PageCard 标题「API Key 管理」可见 → 面板成功挂载。
  const active = page.locator('.ant-tabs-tabpane-active');
  await expect(active.getByText('API Key 管理')).toBeVisible();
  // 「新增 API Key」按钮可见 → 工具条渲染正常（依赖 getSupportedExecutors 不抛错）。
  await expect(active.getByRole('button', { name: '新增 API Key' })).toBeVisible();

  // 加载全程不应弹出 antd 错误 toast（message.error 会渲染 .ant-message-notice-error）。
  await expect(page.locator('.ant-message-notice-error')).toHaveCount(0);
  // 也不应冒出未捕获的 JS 异常。
  expect(pageErrors).toEqual([]);

  console.log('✓ API Key 面板经 providers Facade 加载正常');
});
