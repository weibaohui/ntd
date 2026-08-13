// 096-W4-4-3 冒烟：ExecutorsPanel 拆分（useExecutorAdmin + useExecutorFieldSaver +
// useRunningRecords + RunConfigCard + UsageStatsCard）后的行为等价验证。
// 验证点：执行器 tab 列表/检测按钮/状态开关渲染、运行配置与 AI 使用统计两块拆出的卡片渲染、
// 四个内部 Tab 往返切换（runningTab 状态由 useRunningRecords 托管）、运行配置超时开关本地切换。
// 收口前这些交互由主组件内联 state/handler 驱动，收口后由 hook/子组件驱动，本冒烟为「行为未变」的外层锚点。
import { test, expect } from '@playwright/test';

test.describe('ExecutorsPanel 拆分冒烟', () => {
  test.beforeEach(async ({ page }) => {
    // 直达执行器面板（左侧导航菜单项，hash 路由 /#/executors）。
    await page.goto('http://localhost:18088/#/executors');
    // 等「批量检测」按钮出现即执行器 tab 组件树就绪（useExecutorAdmin 首屏已 loadExecutors）。
    await page.getByRole('button', { name: '批量检测' }).waitFor();
  });

  test('执行器 tab 渲染：列表计数 + 批量检测 + 状态开关 + 检测按钮', async ({ page }) => {
    // 共 N 个执行器计数（useExecutorAdmin.executors 派生）。
    await expect(page.getByText(/共 \d+ 个执行器/)).toBeVisible();
    // 批量检测按钮（useExecutorAdmin.batchDetect 入口）。
    await expect(page.getByRole('button', { name: '批量检测' })).toBeVisible();
    // 状态列开关（行内 enabled 保存，useExecutorFieldSaver.savingExecutor 驱动 loading）。
    await expect(page.locator('.ant-table-tbody .ant-switch').first()).toBeVisible();
    // 操作列「检测」按钮（useExecutorAdmin.detectExecutorByName 入口）。
    await expect(page.getByRole('button', { name: '检测' }).first()).toBeVisible();
  });

  test('运行配置 / AI 使用统计 两块拆出卡片渲染', async ({ page }) => {
    // RunConfigCard：最大并发 + 执行超时（从主组件拆出的全局配置族之一）。
    await expect(page.getByText('运行配置')).toBeVisible();
    await expect(page.getByText('最大并发数')).toBeVisible();
    await expect(page.getByText('执行超时')).toBeVisible();
    // UsageStatsCard：AI 使用统计（拆出的全局配置族之二）。
    await expect(page.getByText('AI 使用统计')).toBeVisible();
  });

  test('运行配置超时开关本地切换（不点保存，仅本地 state）', async ({ page }) => {
    // 执行超时 Switch 切换只改 RunConfigCard 本地 state（handleExecutionTimeoutToggle），
    // 不触发 API（保存需另点「保存」），对 dev 配置无副作用——验证拆出的卡片交互未断。
    const timeoutSwitch = page.locator('.ant-card').filter({ hasText: '运行配置' }).locator('.ant-switch').first();
    const before = await timeoutSwitch.getAttribute('aria-checked');
    await timeoutSwitch.click();
    // 切换后 aria-checked 翻转（本地 state 生效）。
    await expect(timeoutSwitch).not.toHaveAttribute('aria-checked', before ?? '');
  });

  test('四个内部 Tab 往返切换（runningTab 由 useRunningRecords 托管）', async ({ page }) => {
    // API Key tab → 渲染 ProfilesPanel。
    await page.getByRole('tab', { name: 'API Key' }).click();
    // 正在运行 tab → 触发 loadRunningRecords（切到 running 时初始加载）。
    await page.getByRole('tab', { name: '正在运行' }).click();
    // 运行 tab独有：批量停止按钮（useRunningRecords.handleBatchStop 入口）。
    await expect(page.getByRole('button', { name: /批量停止/ })).toBeVisible();
    // 会话 tab → 渲染 SessionManager。
    await page.getByRole('tab', { name: '会话' }).click();
    // 回到执行器 tab，验证 runningTab 状态复位、组件重渲染正常。
    await page.getByRole('tab', { name: '执行器' }).click();
    await expect(page.getByRole('button', { name: '批量检测' })).toBeVisible();
  });
});
