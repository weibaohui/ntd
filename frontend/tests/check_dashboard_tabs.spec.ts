// Dashboard Tab 化重构回归验证。
//
// 覆盖:6 个 Tab 可逐个切换、每个 Tab 渲染出卡片、URL hash 记忆当前 Tab、
// 切换全程控制台无运行时 error。
//
// 注意:playwright.config.ts 的 baseURL=5173 是历史遗留(make dev 实际监听 18088,
// 后端 embedded 模式 serve dist),因此这里用完整 18088 URL,不依赖 baseURL。
import { test, expect } from '@playwright/test';

const DASHBOARD = 'http://localhost:18088/#/dashboard';

// 6 个 Tab 的可访问名(图标 alt + 文案),按展示顺序。
// getByRole name 默认子串匹配,故用文案即可命中「<图标> <文案>」整串。
const TAB_LABELS = ['总览', '任务', '执行', '成本与模型', '自动化', '资源与运维'];

// 等待页面就绪的公共断言:PageCard 标题「仪表盘」可见。
// dashboard 聚合接口可能较慢,给 20s 余量。
async function waitForDashboard(page: import('@playwright/test').Page) {
  await page.goto(DASHBOARD);
  await expect(page.getByText('仪表盘').first()).toBeVisible({ timeout: 20000 });
}

test.describe('Dashboard Tab 化重构', () => {
  test('默认落在总览 Tab 并渲染关键指标与最近执行表', async ({ page }) => {
    await waitForDashboard(page);
    // 总览默认选中(aria-selected=true)。
    await expect(page.getByRole('tab', { name: '总览' })).toHaveAttribute('aria-selected', 'true');
    // 总览两张标志性内容应渲染。
    await expect(page.getByText('关键指标')).toBeVisible();
    await expect(page.getByText('最近执行记录')).toBeVisible();
  });

  test('6 个 Tab 可逐个切换且每个 Tab 都渲染卡片', async ({ page }) => {
    // 收集 console error,切换全程不应有运行时异常(容忍 favicon/404 噪音)。
    const errors: string[] = [];
    page.on('console', (m) => {
      if (m.type() === 'error') errors.push(m.text());
    });

    await waitForDashboard(page);

    for (const label of TAB_LABELS) {
      const tab = page.getByRole('tab', { name: label });
      await tab.click();
      // 切换后该 Tab 高亮。
      await expect(tab).toHaveAttribute('aria-selected', 'true');
      // active panel 内至少一张卡片渲染,防止 Tab 空白崩溃。
      await expect(
        page.locator('.ant-tabs-tabpane-active').locator('.ant-card').first(),
      ).toBeVisible({ timeout: 10000 });
    }

    const realErrors = errors.filter((e) => !e.includes('favicon') && !e.includes('404'));
    expect(realErrors).toEqual([]);
  });

  test('URL hash 记忆当前 Tab', async ({ page }) => {
    await waitForDashboard(page);
    await page.getByRole('tab', { name: '执行' }).click();
    // handleTabChange 调 pushUrl('dashboard', { tab: 'executions' }),
    // 应把 tab=executions 写进 hash,刷新/分享可保持。
    await expect(page).toHaveURL(/tab=executions/);
  });

  test('各 Tab 渲染对应的 P2/P3 新卡片', async ({ page }) => {
    await waitForDashboard(page);

    // 任务 Tab:评分分布(RatingDistCard)
    await page.getByRole('tab', { name: '任务' }).click();
    await expect(page.getByText('评分分布')).toBeVisible({ timeout: 10000 });

    // 成本 Tab:AI 会话(SessionsStatsCard)
    await page.getByRole('tab', { name: '成本与模型' }).click();
    await expect(page.getByText('AI 会话')).toBeVisible({ timeout: 10000 });

    // 自动化 Tab:环路统计 + 飞书监听(消费后端 /api/loops/stats)
    await page.getByRole('tab', { name: '自动化' }).click();
    await expect(page.getByText('环路')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText('飞书监听')).toBeVisible({ timeout: 10000 });

    // 资源与运维 Tab:智能助手 / 系统版本 / 云同步
    await page.getByRole('tab', { name: '资源与运维' }).click();
    await expect(page.getByText('智能助手')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText('系统版本')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText('云同步')).toBeVisible({ timeout: 10000 });
  });
});

// 移动端视口验证:窄屏下 Tab 仍可切换、卡片正常渲染、无 console error。
test.describe('移动端适配(375x812)', () => {
  test.use({ viewport: { width: 375, height: 812 } });

  test('窄屏 6 Tab 可切换且卡片渲染', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', (m) => {
      if (m.type() === 'error') errors.push(m.text());
    });

    await waitForDashboard(page);
    // 遍历各 Tab(移动端 label 已缩短),每个 active panel 都应渲染出卡片。
    // 用 .ant-card 而非具体文案:窄屏 Masonry 单列下文本节点的可见性判定偶有抖动,
    // 卡片元素本身的可见性更稳定,且足以证明 Tab 切换 + 内容渲染成功。
    for (const label of ['任务', '执行', '成本', '自动化', '资源']) {
      await page.getByRole('tab', { name: label }).click();
      await expect(page.locator('.ant-tabs-tabpane-active .ant-card').first()).toBeVisible({
        timeout: 10000,
      });
    }

    const realErrors = errors.filter((e) => !e.includes('favicon') && !e.includes('404'));
    expect(realErrors).toEqual([]);
  });
});
