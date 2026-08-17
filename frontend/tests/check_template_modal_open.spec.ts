// 回归用例（NTD-018 守护）：
// 1. 新建抽屉术语口径——标题/占位必须是「事项」，不得回退旧「任务」文案；
// 2. 「从模板创建」可用性——用户曾反馈"点不开"，此处验证按钮可点（无遮挡）、
//    模板接口 200、弹窗 zIndex 高于抽屉（antd 嵌套 Modal 若被 Drawer 盖住即表现为"点不开"）。
// 运行前提：make dev 已起；baseURL 由 playwright.config.ts 统一提供（18088），用例内不硬编码。

import { test, expect, type Page } from '@playwright/test';

/**
 * 打开事项列表并进入「新建」抽屉，返回钉住的工作空间 id。
 * 工作空间必须钉死：应用 boot 时按 path 排序默认选中某个 ws（本机是 ws3），
 * 不钉会让断言对象随环境漂移（项目 memory 已知坑）。
 * 用 addInitScript 抢在应用初始化前写 localStorage——goto 后再 evaluate 会被
 * useApp 的异步 SELECT_WORKSPACE dispatch 覆盖回默认值（063 用例踩过的竞态）。
 */
async function openCreateDrawer(page: Page): Promise<number> {
  // 取环境里第一个工作空间 id 作为钉住目标；至少存在一个，否则用例无意义。
  const resp = await page.request.get('/api/v1/workspaces');
  const body = await resp.json();
  const wsId: number | undefined = body?.data?.[0]?.id;
  expect(wsId, '环境内至少存在一个工作空间').toBeTruthy();

  // addInitScript 回调序列化到浏览器执行，不能闭包捕获 wsId，必须随 arg 传入。
  await page.addInitScript(
    (ws) => localStorage.setItem('selected_workspace', String(ws)),
    wsId,
  );

  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2500);
  // 新建按钮带 icon（可访问名含 aria-label 前缀，exact 匹配会失败），用子串匹配。
  await page.locator('button', { hasText: '新建' }).first().click();
  await page.waitForTimeout(1200);
  return wsId!;
}

test('新建抽屉标题与占位为「事项」口径（NTD-018）', async ({ page }) => {
  await openCreateDrawer(page);
  // 018 后的权威口径：标题精确匹配，占位文案可见；若有人改回「任务」会在此失败。
  await expect(page.locator('.ant-drawer-title')).toHaveText('创建事项');
  await expect(page.getByPlaceholder('事项标题')).toBeVisible();
  await expect(page.getByPlaceholder('描述完成该事项需要满足的条件...')).toBeVisible();
});

test('从模板创建：按钮无遮挡、接口 200、弹窗浮于抽屉之上', async ({ page }) => {
  await openCreateDrawer(page);

  const tplBtn = page.locator('button', { hasText: '从模板创建' }).first();
  await expect(tplBtn).toBeVisible();

  // 命中测试：按钮中心点最顶层元素必须是按钮自身内容（antd Button 内为 span）。
  // 用户反馈的"点不开"最典型成因就是点击坐标被别的浮层占据。
  const topAtBtn = await tplBtn.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
    return hit ? `${hit.tagName}.${String(hit.className).slice(0, 40)}` : null;
  });
  expect(topAtBtn).toContain('SPAN');

  // 点击后：模板接口必须发出且返回 200（waitForResponse 先挂监听再点击，避免竞态）。
  const tplRespPromise = page.waitForResponse((r) => r.url().includes('todo-templates'));
  await tplBtn.click();
  expect((await tplRespPromise).status()).toBe(200);

  // 弹窗可见，且 zIndex 必须高于抽屉——antd 嵌套 Modal 靠 zIndex 上下文自动抬升，
  // 一旦被 Drawer 盖住，表现就是"点了没反应"。
  const modal = page.locator('.ant-modal-wrap:visible .ant-modal').first();
  await expect(modal).toBeVisible({ timeout: 5000 });
  const above = await modal.evaluate((el) => {
    const wrap = el.closest('.ant-modal-wrap');
    const drawer = document.querySelector('.ant-drawer');
    const wrapZ = wrap ? Number(getComputedStyle(wrap).zIndex) : -1;
    const drawerZ = drawer ? Number(getComputedStyle(drawer).zIndex) : -1;
    return wrapZ > drawerZ;
  });
  expect(above, 'modal zIndex 应高于 drawer').toBe(true);

  // 截图存 test-results/（gitignore 已覆盖，避免二进制误入仓库；也避免相对路径解析出游离目录）。
  await page.screenshot({ path: 'test-results/check_template_modal.png' });
});
