import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test.describe('Process Library 工艺模板库', () => {
  test('navigate to process page and render template cards', async ({ page }) => {
    await page.goto(`${BASE}/#/processes`);
    // 等待页面骨架渲染
    await page.waitForTimeout(1500);

    // 页面标题：028 后 PageCard 标题已由旧的「工艺模板库」改为「工艺」
    // （「工艺模板库」现仅存在于代码注释/帮助文档，不再渲染进 DOM，旧 text= 选择器因此 0 匹配）。
    // 用 .ntd-page-card-title-text 锚定标题文本节点，避免 text=工艺 同时命中「创建工艺」按钮。
    const title = page.locator('.ntd-page-card-title-text', { hasText: '工艺' });
    await expect(title).toBeVisible({ timeout: 5000 });

    // 刷新按钮应存在
    const refreshBtn = page.locator('button:has-text("刷新")');
    await expect(refreshBtn).toBeVisible({ timeout: 5000 });

    // 039：「我的/模板」视图切换应存在（工作空间选择已改为全局左上角选择器，旧的页内选择器断言移除）
    await expect(page.locator('.ant-segmented')).toBeVisible({ timeout: 5000 });

    // 如果已同步工艺模板，应能看到至少一个卡片
    const cards = page.locator('.ant-card');
    const count = await cards.count();
    if (count > 0) {
      // 首张卡片应包含详情/安装按钮
      await expect(page.locator('button:has-text("详情")').first()).toBeVisible();
      await expect(page.locator('button:has-text("安装")').first()).toBeVisible();
      console.log(`工艺模板库渲染了 ${count} 个模板卡片`);
    } else {
      console.log('暂无工艺模板卡片，可能尚未执行 bundled 同步');
    }
  });

  test('process API endpoints return valid structure', async ({ request }) => {
    const listResp = await request.get(`${BASE}/api/bundled/processes`);
    expect(listResp.ok()).toBeTruthy();
    const listBody = await listResp.json();
    expect(Array.isArray(listBody.data)).toBe(true);

    if (listBody.data.length > 0) {
      const first = listBody.data[0];
      expect(first.name).toBeDefined();
      expect(first.display_name).toBeDefined();
      // 040：列表项必须带 guid（按 guid 寻址）
      expect(first.guid).toBeDefined();

      // 040：详情接口按 guid 寻址（name 可重复后不能按 name 取详情）
      const detailResp = await request.get(`${BASE}/api/bundled/processes/${encodeURIComponent(first.guid)}`);
      expect(detailResp.ok()).toBeTruthy();
      const detailBody = await detailResp.json();
      expect(detailBody.data.definition).toBeDefined();
    }
  });
});

// 039：工艺列表「我的/模板」双视图
// 注意：Playwright 每个用例默认开新 browser context，localStorage 天然为空，
// 不能用 addInitScript 清 key——它会在用例内 reload 时再次执行，把刚写入的持久化值抹掉。
test.describe('039 工艺列表我的/模板双视图', () => {
  test('API is_system 过滤：true 只返回系统模板，false 只返回用户模板，不传返回全部', async ({ request }) => {
    const all = (await (await request.get(`${BASE}/api/bundled/processes`)).json()).data;
    const sys = (await (await request.get(`${BASE}/api/bundled/processes?is_system=true`)).json()).data;
    const usr = (await (await request.get(`${BASE}/api/bundled/processes?is_system=false`)).json()).data;

    expect(sys.every((p: { is_system: boolean }) => p.is_system)).toBe(true);
    expect(usr.every((p: { is_system: boolean }) => !p.is_system)).toBe(true);
    // 不传参必须是全量（向后兼容：设置页模板管理依赖全量语义）
    expect(all.length).toBe(sys.length + usr.length);
  });

  test('默认显示「我的」视图，Segmented 两个选项可见', async ({ page }) => {
    await page.goto(`${BASE}/#/processes`);
    await page.waitForTimeout(1500);

    const mine = page.locator('.ant-segmented-item', { hasText: '我的' });
    const tpl = page.locator('.ant-segmented-item', { hasText: '模板' });
    await expect(mine).toBeVisible({ timeout: 5000 });
    await expect(tpl).toBeVisible({ timeout: 5000 });
    // localStorage 无记录时默认选中「我的」
    await expect(mine).toHaveClass(/ant-segmented-item-selected/);
  });

  test('切换「模板」只显示系统工艺且无编辑按钮，localStorage 持久化，刷新后保持', async ({ page }) => {
    await page.goto(`${BASE}/#/processes`);
    await page.waitForTimeout(1500);

    await page.locator('.ant-segmented-item', { hasText: '模板' }).click();
    await page.waitForTimeout(1000);

    // 视图选择写入 localStorage
    const stored = await page.evaluate(() => localStorage.getItem('ntd_process_list_scope'));
    expect(stored).toBe('template');

    // 模板视图的卡片不应出现「编辑」按钮（系统工艺只读）
    const cards = page.locator('.ant-card');
    if ((await cards.count()) > 0) {
      await expect(page.locator('.ant-card button:has-text("编辑")')).toHaveCount(0);
    }

    // 刷新后应保持「模板」视图
    await page.reload();
    await page.waitForTimeout(1500);
    const tpl = page.locator('.ant-segmented-item', { hasText: '模板' });
    await expect(tpl).toHaveClass(/ant-segmented-item-selected/);
  });

  test('「我的」视图的卡片带编辑按钮（存在用户工艺时）', async ({ page }) => {
    // 先查 API 确认是否存在用户工艺，没有则跳过断言（CI 全新库允许为空）
    const usr = await (await page.request.get(`${BASE}/api/bundled/processes?is_system=false`)).json();
    await page.goto(`${BASE}/#/processes`);
    await page.waitForTimeout(1500);

    if (usr.data.length > 0) {
      await expect(page.locator('.ant-card button:has-text("编辑")').first()).toBeVisible({ timeout: 5000 });
    } else {
      // 「我的」空态应引导创建工艺
      await expect(page.locator('text=暂无自定义工艺')).toBeVisible({ timeout: 5000 });
    }
  });
});

// 040：模板「复制为我的工艺」——副本换新 guid、与模板同名共存、原模板不消失
test.describe('040 工艺模板复制为我的工艺', () => {
  test('复制后模板仍在、我的视图多出同名副本（guid 不同），并清理副本', async ({ request }) => {
    const sys = (await (await request.get(`${BASE}/api/bundled/processes?is_system=true`)).json()).data;
    test.skip(!sys || sys.length === 0, '无系统模板，跳过（需先同步 ntd-resource）');

    const src = sys[0];
    const copyResp = await request.post(`${BASE}/api/v1/processes/${encodeURIComponent(src.guid)}/copy-to-user`);
    expect(copyResp.ok()).toBeTruthy();
    const copy = (await copyResp.json()).data;
    // 副本是新 guid、同名
    expect(copy.guid).toBeDefined();
    expect(copy.guid).not.toBe(src.guid);
    expect(copy.name).toBe(src.name);

    // 模板视图：原模板仍在（核心回归——旧同名复制会让模板消失）
    const sysAfter = (await (await request.get(`${BASE}/api/bundled/processes?is_system=true`)).json()).data;
    expect(sysAfter.some((p: { guid: string }) => p.guid === src.guid)).toBe(true);

    // 我的视图：副本以新 guid 出现
    const usrAfter = (await (await request.get(`${BASE}/api/bundled/processes?is_system=false`)).json()).data;
    expect(usrAfter.some((p: { guid: string }) => p.guid === copy.guid)).toBe(true);

    // 清理副本，避免污染开发库
    const delResp = await request.delete(`${BASE}/api/v1/processes/${encodeURIComponent(copy.guid)}`);
    expect([200, 204]).toContain(delResp.status());
  });
});
