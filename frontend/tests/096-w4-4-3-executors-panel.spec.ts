// 096-W4-4-3 冒烟：ExecutorsPanel 拆分（useExecutorAdmin + useExecutorFieldSaver +
// useRunningRecords + ExecutorsTable + RunConfigCard + UsageStatsCard + RunningRecordsTable）后的行为等价验证。
// 两层覆盖：
//  ① 渲染层——列表/检测/状态开关/两配置卡片/超时开关本地切换/四 Tab 往返；
//  ② 交互层（对应 101 设计文档验证门禁「行内编辑保存/检测按钮/默认切换」）——
//     检测按钮点击落结果、二进制路径行内编辑保存跨重载持久、设为默认翻转，均带状态还原。
// 收口前这些交互由主组件内联 state/handler 驱动，收口后由 hook/子组件驱动，本冒烟为「行为未变」的外层锚点。
import { test, expect } from '@playwright/test';

// 执行器 tab 数据为全局配置（不按 workspace 归属），故 executors-tab 用例无需 pin selected_workspace。
test.describe('ExecutorsPanel 拆分冒烟', () => {
  test.beforeEach(async ({ page }) => {
    // 直达执行器面板（左侧导航菜单项，hash 路由 /#/executors）。
    await page.goto('http://localhost:18088/#/executors');
    // 等「批量检测」按钮出现即执行器 tab 组件树就绪（useExecutorAdmin 首屏已 loadExecutors）。
    await page.getByRole('button', { name: '批量检测' }).waitFor();
  });

  test('渲染：列表计数 + 批量检测 + 状态开关 + 检测按钮', async ({ page }) => {
    await expect(page.getByText(/共 \d+ 个执行器/)).toBeVisible();
    await expect(page.getByRole('button', { name: '批量检测' })).toBeVisible();
    // 状态列开关（行内 enabled 保存，useExecutorFieldSaver.savingExecutor 驱动 loading）。
    await expect(page.locator('.ant-table-tbody .ant-switch').first()).toBeVisible();
    // 操作列「检测」按钮（useExecutorAdmin.detectExecutorByName 入口）。
    await expect(page.getByRole('button', { name: '检测' }).first()).toBeVisible();
  });

  test('渲染：运行配置 / AI 使用统计 两块拆出卡片', async ({ page }) => {
    await expect(page.getByText('运行配置')).toBeVisible();
    await expect(page.getByText('最大并发数')).toBeVisible();
    await expect(page.getByText('执行超时')).toBeVisible();
    await expect(page.getByText('AI 使用统计')).toBeVisible();
  });

  test('运行配置超时开关本地切换（不点保存，仅本地 state）', async ({ page }) => {
    // 执行超时 Switch 切换只改 RunConfigCard 本地 state，不触发 API，对 dev 配置无副作用。
    const timeoutSwitch = page.locator('.ant-card').filter({ hasText: '运行配置' }).locator('.ant-switch').first();
    const before = await timeoutSwitch.getAttribute('aria-checked');
    await timeoutSwitch.click();
    await expect(timeoutSwitch).not.toHaveAttribute('aria-checked', before ?? '');
  });

  test('交互：四个内部 Tab 往返切换（runningTab 由 useRunningRecords 托管）', async ({ page }) => {
    await page.getByRole('tab', { name: 'API Key' }).click();
    await page.getByRole('tab', { name: '正在运行' }).click();
    // 运行 tab 独有：批量停止按钮（RunningRecordsTable.handleBatchStop 入口）。
    await expect(page.getByRole('button', { name: /批量停止/ })).toBeVisible();
    await page.getByRole('tab', { name: '会话' }).click();
    await page.getByRole('tab', { name: '执行器' }).click();
    await expect(page.getByRole('button', { name: '批量检测' })).toBeVisible();
  });

  test('交互：检测按钮点击 → 检测状态落结果（detectExecutorByName 端到端）', async ({ page }) => {
    const row0 = page.locator('.ant-table-tbody tr.ant-table-row').first();
    // 首行「检测」按钮：点击 + 等后端 POST .../detect 返回（detectExecutorByName → detectExecutor）。
    const detectBtn = row0.getByRole('button', { name: '检测' });
    const [resp] = await Promise.all([
      page.waitForResponse((r) => r.url().includes('/detect') && r.request().method() === 'POST'),
      detectBtn.click(),
    ]);
    expect(resp.ok()).toBeTruthy();
    // 检测状态由「未检测」翻为「可用」或「不可用」（取决于本机是否装了该二进制，两种都算通过）。
    await expect(row0.getByText(/可用|不可用/)).toBeVisible({ timeout: 10000 });
    // 检测仅写内存 detectResults，无配置副作用，无需还原。
  });

  test('交互：二进制路径行内编辑保存 → 跨重载持久（saveExecutorField 端到端）', async ({ page }) => {
    const pathInput = page.getByPlaceholder('二进制路径或命令名').first();
    const original = await pathInput.inputValue();
    const sentinel = `${original || '/x'}-w43-smoke`;

    // 改值 + 失焦触发 onBlur → saveExecutorField → PUT /api/v1/executors/{name}。
    await pathInput.fill(sentinel);
    await Promise.all([
      page.waitForResponse((r) => r.url().includes('/api/v1/executors/') && r.request().method() === 'PUT'),
      pathInput.blur(),
    ]);
    // 重载：重新 mount → loadExecutors 从 DB 读回，验证保存确已落库。
    await page.reload();
    await page.getByRole('button', { name: '批量检测' }).waitFor();
    await expect(page.getByPlaceholder('二进制路径或命令名').first()).toHaveValue(sentinel);

    // 还原原值，保证 dev 配置不被冒烟污染。
    const restore = page.getByPlaceholder('二进制路径或命令名').first();
    await restore.fill(original);
    await Promise.all([
      page.waitForResponse((r) => r.url().includes('/api/v1/executors/') && r.request().method() === 'PUT'),
      restore.blur(),
    ]);
    await page.reload();
    await page.getByRole('button', { name: '批量检测' }).waitFor();
    await expect(page.getByPlaceholder('二进制路径或命令名').first()).toHaveValue(original);
  });

  test('交互：设为默认翻转 + 还原（setAsDefault 全表重算 is_default）', async ({ page }) => {
    const tbody = page.locator('.ant-table-tbody');
    // 按钮内含图标，accessible name 形如「star 默认」/「star 设为默认」，按文案区分不可靠；
    // 改用：原默认显示名从 API 读，候选行从「设为默认」按钮经 xpath 上溯定位，翻转以「设为默认」按钮消失判定。
    const defaultApi = await page.request.get('http://localhost:18088/api/v1/executors/default');
    const body = defaultApi.ok() ? await defaultApi.json() : null;
    const originalDefault = (body?.data ?? body)?.display_name ?? '';

    // 非默认候选：第一个「设为默认」按钮；经 xpath 上溯到其所在行，取显示名（用名 pin 行，避免按钮翻转后定位漂移）。
    const candidateBtn = tbody.getByRole('button', { name: /设为默认/ }).first();
    const candidateName = (await candidateBtn.locator('xpath=ancestor::tr[contains(@class,"ant-table-row")]')
      .locator('td').nth(1).innerText()).trim();

    // 点击设为默认 → PUT .../default；按显示名 pin 住候选行，断言其「设为默认」消失、「默认」出现。
    await Promise.all([
      page.waitForResponse((r) => r.url().includes('/default') && r.request().method() === 'PUT'),
      candidateBtn.click(),
    ]);
    const candidateRowByName = tbody.locator('tr.ant-table-row').filter({ hasText: candidateName }).first();
    await expect(candidateRowByName.getByRole('button', { name: /设为默认/ })).toHaveCount(0);
    await expect(candidateRowByName.getByRole('button', { name: /默认/ })).toBeVisible({ timeout: 5000 });

    // 还原：原默认执行器此时已丢默认（按钮变「设为默认」），点回；无原默认则跳过（候选成为新默认，合法）。
    if (originalDefault && originalDefault !== candidateName) {
      const restoreRow = tbody.locator('tr.ant-table-row').filter({ hasText: originalDefault }).first();
      await Promise.all([
        page.waitForResponse((r) => r.url().includes('/default') && r.request().method() === 'PUT'),
        restoreRow.getByRole('button', { name: /设为默认/ }).click(),
      ]);
      await expect(restoreRow.getByRole('button', { name: /默认/ })).toBeVisible({ timeout: 5000 });
    }
  });
});
