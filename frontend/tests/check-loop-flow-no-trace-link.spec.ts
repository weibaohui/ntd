// 验证：展开「执行历史」不应再让上方流程图的高亮/淡出变化（trace 联动已移除）。
// 通过 API 找一个有 steps 的 loop，从 ?loop=<id> 路由直接进入详情，对比展开执行历史前后的 SVG 透明度。
import { test, expect } from '@playwright/test';

const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';
const DEV_URL = 'http://localhost:18088';

test('展开执行历史不应影响流程图渲染', async ({ page }) => {
  // 找一个至少有 1 个 step 的 loop（通过 API 直接筛选）
  const listRes = await page.request.get(`${BACKEND_URL}/api/loops?page=1&limit=50`);
  const listJson = await listRes.json();
  const loops: Array<{ id: number; step_count: number }> = listJson?.data ?? [];
  const candidate = loops.find((l) => l.step_count > 0);
  test.skip(!candidate, '没有可用的带 step 的 loop，跳过用例');
  const loopId = candidate!.id;

  // 直接从 URL 进入 loop 详情
  await page.goto(`${DEV_URL}/?loop=${loopId}`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);

  // 流程图整体在一个 svg 节点里
  const svg = page.locator('svg').first();
  await expect(svg).toBeVisible();

  // 采样展开前 SVG 内部各 <g opacity> 的值；移除 trace 后应全为 1。
  const sampleOpacity = async () => svg.evaluate((root) =>
    Array.from(root.querySelectorAll('g[opacity]')).map((g) =>
      parseFloat(g.getAttribute('opacity') || '1'),
    ),
  );

  const before = await sampleOpacity();

  // 展开「执行历史」折叠面板
  const historyHeader = page.getByText(/执行历史/).first();
  if (await historyHeader.count() > 0) {
    await historyHeader.click();
    await page.waitForTimeout(800);
  }

  const after = await sampleOpacity();

  // 流程图内不应再出现半透明节点/边 —— 全部应保持 1
  const dimmed = after.filter((o) => o < 1);
  expect(dimmed, `发现 ${dimmed.length} 个被淡化的 g：${JSON.stringify(after)}`).toEqual([]);
  // 展开前后 g 数量应当一致（不再因 trace 触发额外的重渲染分支）
  expect(after.length).toBe(before.length);

  // 截图留档，便于人工核对（不入 git，输出在 test-results/）
  await page.screenshot({ path: `test-results/loop-flow-no-trace-link-${loopId}.png`, fullPage: false });
});
