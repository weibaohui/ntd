import { test, expect, type Page } from '@playwright/test';

/**
 * ADR-005 / H5：ProfilesPanel 直连 fetch 收敛为 providers Facade 后的冒烟验证。
 *
 * API Key 面板挂在「执行器」页的「API Key」tab 下。进入该 tab 会触发 ProfilesPanel 的
 * 加载流程（listProviders + getSupportedExecutors + 对每个 provider 并发 getProvider），
 * 新增/删除/导出则分别走 createProvider/deleteProvider/exportProviders。这些调用现已全部
 * 走 providers Facade（client.ts 的 axios 实例），端点/解包写错会在以下用例里暴露。
 *
 * 注：provider 是全局服务端配置（非 workspace 维度），故不按 workspace 固定选择，
 * 与 verify_executor_tabs.spec.ts 的导航方式保持一致。
 */

const API_URL = 'http://localhost:18088/api/v1';
// 测试用 provider 标识符——仅字母数字中划线，符合表单 /^[a-zA-Z0-9_-]+$/ 校验。
// 用固定名便于跨用例自愈清理（见 ensureAbsent）。
const PW_NAME = 'pw-facade-test';
// 卡片显示名，用于在列表里定位新建的卡片与删除后断言消失。
const DISPLAY = 'PW-Facade-Test';

/** 进入「执行器 → API Key」面板，返回当前激活的 tabpane 定位器（Modal 由 portal 渲染，仍用 page scope）。 */
async function gotoApiKeyPanel(page: Page) {
  await page.goto('http://localhost:18088');
  await page.waitForLoadState('networkidle');
  await page.click('[data-testid="left-rail-settings_executors"]');
  await page.waitForTimeout(800);
  // 切到「API Key」tab，挂载 ProfilesPanel 并触发加载请求。
  const tab = page.locator('.ant-tabs-tab').filter({ hasText: 'API Key' });
  await expect(tab).toBeVisible();
  await tab.click();
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(500);
  return page.locator('.ant-tabs-tabpane-active');
}

/** 直接 DELETE 清掉测试 provider，忽略 404/网络错。重跑自愈，避免重名导致 create 被后端拒绝。 */
async function ensureAbsent(page: Page) {
  await page.request.delete(`${API_URL}/providers/${PW_NAME}`).catch(() => { /* 不存在即已干净 */ });
}

test('加载：API Key 面板经 providers Facade 加载正常', async ({ page }) => {
  // 采集未捕获异常，兜住 Facade 调用失败被静默吞掉的情况。
  const pageErrors: string[] = [];
  page.on('pageerror', (e) => pageErrors.push(e.message));

  const active = await gotoApiKeyPanel(page);
  // PageCard 标题与「新增 API Key」按钮可见 → 面板挂载、getSupportedExecutors 未抛错。
  await expect(active.getByText('API Key 管理')).toBeVisible();
  await expect(active.getByRole('button', { name: '新增 API Key' })).toBeVisible();
  // 加载全程不应弹错误 toast，也不应有未捕获 JS 异常。
  await expect(page.locator('.ant-message-notice-error')).toHaveCount(0);
  expect(pageErrors).toEqual([]);

  console.log('✓ 加载：API Key 面板经 providers Facade 加载正常');
});

test('新增→删除：经 Facade 创建后列表出现，删除后消失', async ({ page }) => {
  await ensureAbsent(page);
  const active = await gotoApiKeyPanel(page);

  // —— 新增：打开 Modal，按 placeholder 定位填写（antd Form.Item label 不挂 for，placeholder 最稳）——
  await active.getByRole('button', { name: '新增 API Key' }).click();
  // antd v6 的 Modal 无 .ant-modal-content，最内层容器就是 .ant-modal（含 body+footer）。
  const modal = page.locator('.ant-modal').last();
  // getByPlaceholder 默认大小写不敏感+子串匹配，'如: DeepSeek' 会误命中 '如: deepseek-anthropic'，故全用 exact。
  await modal.getByPlaceholder('如: deepseek-anthropic', { exact: true }).fill(PW_NAME);
  await modal.getByPlaceholder('如: DeepSeek', { exact: true }).fill(DISPLAY);
  await modal.getByPlaceholder('sk-xxx', { exact: true }).fill('sk-pw-test-key');
  await modal.getByPlaceholder('https://api.example.com/v1', { exact: true }).fill('https://api.example.com/v1');
  // 加一个模型，更接近真实 provider；模型区「添加」按钮与工具条按钮文案不冲突（Modal 内 scope）。
  await modal.getByRole('button', { name: '添加' }).click();
  await modal.getByPlaceholder('模型标识', { exact: true }).fill('pw-test-model');
  // antd 对纯文本 2 字按钮自动插空格（footer 的"保 存"），用正则容忍。
  await modal.getByRole('button', { name: /保\s*存/ }).click();

  // createProvider 经 Facade：拦截器放行 → 组件 message.success('已创建') + load() 重拉列表。
  await expect(page.locator('.ant-message-notice').filter({ hasText: '已创建' })).toBeVisible();
  // 保存成功后 createVisible/editVisible 置 false，Modal 关闭（antd 默认不卸载仅隐藏，故断言不可见而非计数归 0）。
  await expect(page.locator('.ant-modal')).not.toBeVisible();
  await page.waitForLoadState('networkidle');

  // 新卡片出现，说明 list→detail 链路把它拉回来了。
  const card = active.locator('.ant-card').filter({ hasText: DISPLAY });
  await expect(card).toBeVisible();

  // —— 删除：卡片内唯一的 danger 按钮即删除（编辑=link 无 danger，应用=primary 无 danger）——
  await card.locator('.ant-btn-dangerous').first().click();
  const pop = page.locator('.ant-popconfirm');
  // Popconfirm 的"删除"是纯文本 2 字按钮，antd 自动插空格成"删 除"。
  await pop.getByRole('button', { name: /删\s*除/ }).click();
  // deleteProvider 经 Facade 成功 → message.success('已删除') + load()。
  await expect(page.locator('.ant-message-notice').filter({ hasText: '已删除' })).toBeVisible();
  await page.waitForLoadState('networkidle');
  await expect(active.locator('.ant-card').filter({ hasText: DISPLAY })).toHaveCount(0);

  console.log('✓ 新增→删除：经 Facade 创建后列表出现，删除后消失');
});

test('导出：经 Facade 拉取 YAML 并触发下载', async ({ page }) => {
  const active = await gotoApiKeyPanel(page);
  // handleExport 拿到 YAML 后用 <a download> 触发下载；先挂监听再点击。
  const downloadPromise = page.waitForEvent('download');
  await active.getByRole('button', { name: '导出' }).click();
  const download = await downloadPromise;
  // 文件名由 handleExport 拼成 ntd-providers-YYYY-MM-DD.yaml。
  expect(download.suggestedFilename()).toMatch(/ntd-providers-.*\.yaml$/);
  // exportProviders() 成功拿到 YAML 文本（不走 unwrap 的 text 通路）才会进 message.success。
  await expect(page.locator('.ant-message-notice').filter({ hasText: '已导出' })).toBeVisible();

  console.log('✓ 导出：经 Facade 拉取 YAML 并触发下载');
});
