import { test, expect } from '@playwright/test';

/**
 * 验证 ProposalButton 与 ActionButton 对齐后的前置 Drawer：
 * 点击「生成建议」应先弹出 Drawer 展示 prompt / 执行器选择 / 参数预览 / 「执行」按钮，
 * 而不是直接触发执行。不真点「执行」（pi 慢且不确定），只校验前置 UI。
 */
const BLACKBOARD_URL = 'http://localhost:18088/#/blackboard?workspace=1';

test.describe('生成建议前置 Drawer（与 ActionButton 对齐）', () => {
  test('点击生成建议先弹 Drawer 展示 prompt 与执行器，而非直接执行', async ({ page }) => {
    await page.goto(BLACKBOARD_URL);
    await expect(page.locator('.ant-menu-item').first()).toBeVisible({ timeout: 15000 });

    // 点「生成建议」触发按钮
    await page.getByRole('button', { name: '生成建议' }).click();

    // 应弹出 Drawer，标题为「生成 Todo 建议」
    const drawer = page.locator('.ant-drawer-content, .ant-drawer').filter({ hasText: '生成 Todo 建议' });
    await expect(drawer.first()).toBeVisible({ timeout: 5000 });

    // Drawer 内有可编辑 prompt 文本框（含拆解专家关键字）
    const promptArea = drawer.locator('textarea').first();
    await expect(promptArea).toBeVisible();
    await expect(promptArea).toHaveValue(/任务拆解专家/);

    // 有「执行」按钮，且此时未开始执行（按钮可点、无 loading）
    // antd zhCN locale 对两字中文按钮插空格（执行→"执 行"），用 \s* 兼容
    const execBtn = drawer.getByRole('button', { name: /执\s*行/ });
    await expect(execBtn).toBeVisible();
    await expect(execBtn).not.toBeDisabled();

    // 关闭 Drawer，不执行
    await page.keyboard.press('Escape');
  });

  test('执行中渲染实时日志流（ChatView），而非裸 Spin', async ({ page }) => {
    await page.goto(BLACKBOARD_URL);
    await expect(page.locator('.ant-menu-item').first()).toBeVisible({ timeout: 15000 });

    // 点「生成建议」→ 弹 Drawer → 点「执行」触发后端（dev 环境 pi）
    await page.getByRole('button', { name: '生成建议' }).click();
    const drawer = page.locator('.ant-drawer-content, .ant-drawer').filter({ hasText: '生成 Todo 建议' });
    await expect(drawer.first()).toBeVisible({ timeout: 5000 });
    await drawer.getByRole('button', { name: /执\s*行/ }).click();

    // execute() 同步置 executing → 立刻渲染执行态。执行态用 ChatView 承载实时日志：
    // 还没收到 WS Output 时是 .chat-empty（等待AI响应），收到日志后变 .chat-container。
    // 二者出现其一即证明 ChatView 已接入（取代旧的黑盒 Spin）。
    await expect(drawer.locator('.chat-empty, .chat-container').first()).toBeVisible({ timeout: 10000 });
    // 执行态头部仍保留「AI 正在处理中...」状态文案
    await expect(drawer.getByText('AI 正在处理中...')).toBeVisible();
    // 不等执行完成，直接结束用例（pi 在后台继续跑，dev 环境可接受）
  });
});
