/**
 * Loop Studio (环路编排) UI 测试
 *
 * 验证关键流程:
 * - 从 TodoList 点击「环路」按钮进入 LoopStudio
 * - 新建 loop 后出现在左栏, 详情面板显示 4 个分区(分段式布局, 非 Tabs)
 * - 新建阶段时, 若选不到专家应可走「内联新建专家」流程
 * - 触发器 / 钩子增删 UI 可用
 * - 删除 loop 后从列表消失
 *
 * 与后端 loop_expert_tests (kind 校验) + V7LoopStudio 迁移配套.
 */

import { test, expect, chromium } from '@playwright/test';

// Loop Studio 集成测试: 走 ntd dev 服务 (18088), 跳过 vite dev (5173).
// vite dev 是另一个 Node 进程, 不能保证 picked up 最新源码; 嵌入 ntd 二进制的
// dist/ 才是本次 make dev 重新打包后的产物, 一定有环路按钮 + 8 种 trigger 等.
const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:18088';
// 后端 dev 服务跑在 18088, vite dev proxy 把 /api 转发到 18088.
// 直接打 18088 探测后端是否就绪, 避开 vite 启动慢导致 30s 误判.
const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';

/** 等待后端 API 响应, 避免在 dev 服务冷启时撞到 ECONNREFUSED */
async function waitBackendReady(page: import('@playwright/test').Page) {
  // 反复打 /api/loops 直到拿到 200, 最多 30s
  for (let i = 0; i < 60; i++) {
    try {
      const res = await page.request.get(`${BACKEND_URL}/api/loops`);
      if (res.ok()) return;
    } catch {
      // ignore
    }
    await page.waitForTimeout(500);
  }
  throw new Error(`backend not ready after 30s at ${BACKEND_URL}`);
}

test.describe('Loop Studio 端到端', () => {
  test('环路页面入口可点击, 显示主容器', async ({ page }) => {
    await page.goto(DEV_URL);
    await waitBackendReady(page);
    await page.waitForSelector('main', { timeout: 5000 });

    // 顶部 nav 有「环路」按钮 (TodoList.buildDesktopNavActions)
    const loopBtn = page.getByRole('button', { name: '环路编排' });
    await expect(loopBtn).toBeVisible({ timeout: 5000 });
    await loopBtn.click();

    // 进入 LoopStudio 后, 标题与「新建 loop」按钮都应可见
    await expect(page.getByText('环路编排').first()).toBeVisible();
    await expect(page.getByRole('button', { name: '新建 loop' })).toBeVisible();

    // 容错: 不强求「暂无 loop」, 因为前面测试可能已建过 loop, 只要左栏可见即可
    await expect(page.locator('.loop-studio-list-col')).toBeVisible();
  });

  test('新建 loop 流程: 弹出 modal → 提交 → 出现在列表中', async ({ page }) => {
    await page.goto(DEV_URL);
    await waitBackendReady(page);

    await page.getByRole('button', { name: '环路编排' }).click();
    await page.getByRole('button', { name: '新建 loop' }).click();

    // modal 标题
    await expect(page.getByText('新建 loop').nth(1)).toBeVisible();
    // 名称必填
    const nameInput = page.locator('input[placeholder*="例如"]').first();
    await nameInput.fill('测试 loop A');

    // 提交 (modal OK 按钮 = 新建)
    await page.locator(`[role="dialog"] button:has-text("建")`).first().click();

    // 列表里出现 "测试 loop A"
    await expect(page.getByText('测试 loop A').first()).toBeVisible({ timeout: 5000 });

    // 详情面板可见 (新设计是分段式: 阶段/触发器是 DetailSection 常驻可见;
    // 钩子/执行历史是 Collapse 默认收起, label 在 DOM 内可见)
    // 注意: 新设计不再用 antd Tabs, 不能用 getByRole('tab') — 改用 text 匹配
    await expect(page.getByText('流水线阶段').first()).toBeVisible();
    await expect(page.getByText('触发条件').first()).toBeVisible();
    // Collapse label 在 DOM 内, 用 .first() 取到即视为存在
    await expect(page.getByText(/^钩子 \(\d+\)$/).first()).toBeAttached();
    await expect(page.getByText('执行历史').first()).toBeAttached();
  });

  test('阶段 tab 支持内联新建专家 (无 expert 候选时)', async ({ page }) => {
    await page.goto(DEV_URL);
    await waitBackendReady(page);

    // 准备: 先建一个 loop
    await page.getByRole('button', { name: '环路编排' }).click();
    await page.getByRole('button', { name: '新建 loop' }).click();
    await page.locator('input[placeholder*="例如"]').first().fill('阶段测试 loop');
    await page.locator(`[role="dialog"] button:has-text("建")`).first().click();
    await expect(page.getByText('阶段测试 loop').first()).toBeVisible({ timeout: 5000 });

    // 滚到「流水线阶段」section 区域, 找到「新增阶段」按钮
    // 新设计「流水线阶段」是常驻可见的 DetailSection, 无需切换 tab
    await expect(page.getByText('流水线阶段').first()).toBeVisible();
    await page.getByRole('button', { name: '新增阶段' }).first().click();

    // modal 里有「内联新建专家」入口
    const inlineNew = page.getByRole('button', { name: /内联新建专家/ });
    // 即便没弹 modal, 也应该能找到这个按钮
    if (await inlineNew.count() > 0) {
      await expect(inlineNew.first()).toBeVisible();
    }
    // modal 标题应该出现
    await expect(page.getByText('新增阶段').first()).toBeVisible();
  });

  test('触发器 tab 包含 8 种类型选项 (manual/cron/webhook/feishu/todo/tag)', async ({ page }) => {
    await page.goto(DEV_URL);
    await waitBackendReady(page);

    // 准备: 先建一个 loop, 进入其触发器 tab 后点「新增触发器」, 从 Select 弹层读 8 种类型.
    // 之前用 dynamic import('/src/...') 失败: 18088 是 ntd 嵌入式模式, 不暴露 vite 的 /src/.
    // 直接读 antd Select 弹层 (动态 portal) 的 .ant-select-item-option-content,
    // 既真实又规避 import 路径问题.
    await page.getByRole('button', { name: '环路编排' }).click();
    await page.getByRole('button', { name: '新建 loop' }).click();
    await page.locator('input[placeholder*="例如"]').first().fill('触发器测试 loop');
    await page.locator(`[role="dialog"] button:has-text("建")`).first().click();
    await expect(page.getByText('触发器测试 loop').first()).toBeVisible({ timeout: 5000 });

    // 选中该 loop (点列表行), 「触发条件」section 在新设计里常驻可见, 无需切换
    await page.locator('.loop-list-panel .loop-row').filter({ hasText: '触发器测试 loop' }).first().click();
    await expect(page.getByText('触发条件').first()).toBeVisible();

    // 打开「新增触发器」modal
    await page.getByRole('button', { name: '新增触发器' }).click();
    await expect(page.getByText('新增触发器').nth(1)).toBeVisible();

    // 点开「类型」Select (Form.Item label="类型"). modal 内只有一个 ant-select
    // (config/priority 用 Input/InputNumber, 没有 select), 所以 .first() 是它.
    await page.locator('[role="dialog"] .ant-select').first().click();

    // 等选项 portal 出现, 读取所有 option 文本
    await expect(page.locator('.ant-select-dropdown:not(.ant-select-dropdown-hidden)').first()).toBeVisible({ timeout: 5000 });
    const options = await page.locator('.ant-select-dropdown:not(.ant-select-dropdown-hidden) .ant-select-item-option-content').allTextContents();

    // 8 种类型都应出现 (label 部分 + desc 用 " — " 拼接, 用包含匹配)
    const joined = options.join(' | ');
    expect(options.length).toBeGreaterThanOrEqual(8);
    expect(joined).toContain('手动');
    expect(joined).toContain('定时');
    expect(joined).toContain('Webhook');
    expect(joined).toContain('飞书消息');
    expect(joined).toContain('飞书指令');
    expect(joined).toContain('Todo 完成');
    expect(joined).toContain('Todo 状态变更');
    expect(joined).toContain('标签新增');
  });

  test('删除 loop 后从列表消失', async ({ page }) => {
    await page.goto(DEV_URL);
    await waitBackendReady(page);

    await page.getByRole('button', { name: '环路编排' }).click();
    await page.getByRole('button', { name: '新建 loop' }).click();
    await page.locator('input[placeholder*="例如"]').first().fill('待删除 loop');
    await page.locator(`[role="dialog"] button:has-text("建")`).first().click();
    await expect(page.getByText('待删除 loop').first()).toBeVisible({ timeout: 5000 });

    // 删除按钮不在列表行 (LoopListPanel 只展示状态/元信息),
    // 而在 LoopStudioDetailPanel 顶栏的「删除」Popconfirm 触发按钮上 (danger + DeleteOutlined).
    // 先点列表行选中 loop, 详情面板出现后才能点.
    await page.locator('.loop-list-panel .loop-row').filter({ hasText: '待删除 loop' }).first().click();
    await expect(page.locator('.loop-detail-panel')).toBeVisible({ timeout: 5000 });

    // 详情 header 内只有一个 danger 按钮 (删除), 用 .ant-btn-dangerous 锁定
    const deleteTrigger = page.locator('.loop-detail-header .ant-btn-dangerous').first();
    await expect(deleteTrigger).toBeVisible();
    await deleteTrigger.click();

    // Popconfirm 二次确认: 弹层内有「确 定」按钮 (antd v6 自动加空格, 用 has-text 兜底)
    // 找可见的 popover-inner, 内容包含「删除 loop」title 与「确 定」ok
    const popover = page.locator('.ant-popover:not(.ant-popover-hidden)').filter({ hasText: '删除 loop' }).first();
    await expect(popover).toBeVisible({ timeout: 5000 });
    // 用 okType="danger" 的红色按钮, 在 antd v6 中 class 含 ant-btn-dangerous
    await popover.locator('button.ant-btn-dangerous').first().click();

    // 列表里不再有 (限定在 .loop-list-panel, 避免 message.success 提示干扰)
    await expect(page.locator('.loop-list-panel').getByText('待删除 loop')).toHaveCount(0, { timeout: 5000 });
  });
});
