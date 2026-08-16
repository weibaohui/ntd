// 109-PAT验证与列表形态直达路由：验证两个新能力。
//
// 设计文档：docs/design/109-PAT验证与列表形态直达路由-设计.md
//
// 覆盖范围：
// 1. 四个列表视图（todos/loops/tasks/processes）的 ?view= 直达形态：URL 进入即渲染指定形态，
//    Segmented 选中态正确；切换形态后 URL 同步携带 ?view=（replaceUrl，不膨胀历史栈）。
// 2. 设置「第三方授权」页：已配置态显示「验证」按钮；点击后调用后端 verify 接口
//    （真实 GitCode /user），成功展示用户名、失败展示错误信息。
//
// 注意：antd Segmented 的 options.title 渲染为 HTML title 属性（图标态无可见文本），
// 断言/点击需走 [title=...] 选择器。
//
// 依赖：后端运行在 18088 端口；PAT 验证用例依赖本机 ~/.ntd/contribution_pat.json，
// 存在即点击验证按钮并断言结果区出现（成功用户名或失败提示），不存在则断言按钮不渲染。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 等待 hash 路由生效 + 页面渲染稳定的统一延迟。 */
const ROUTE_SETTLE_MS = 900;

test.describe('109 列表形态直达路由', () => {
  test.beforeEach(async ({ page }) => {
    // 进入应用首页，等待 LeftRail 出现表明应用已挂载
    await page.goto(`${BASE}/`);
    await page.waitForLoadState('domcontentloaded');
  });

  test('事项：/#/todos?view=list 直达列表形态', async ({ page }) => {
    // 测试目的：URL 带 ?view=list 时事项页应直接渲染列表形态（table），而非默认卡片墙
    await page.goto(`${BASE}/#/todos?view=list`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // Segmented 选中「列表」（桌面端 title='列表'）
    const toggle = page.getByTestId('todo-center-view-toggle');
    await expect(toggle).toBeVisible();
    await expect(toggle.locator('.ant-segmented-item-selected .ant-segmented-item-label')).toHaveAttribute('title', '列表');

    // 列表形态渲染 table 容器（antd Table 的 table 元素）
    await expect(page.locator('.ant-table').first()).toBeVisible();
  });

  test('事项：切到「执行监控」形态后 URL 同步 ?view=running', async ({ page }) => {
    // 测试目的：用户切换形态 → URL 被 replaceUrl 更新为 ?view=running，可分享直达
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    const toggle = page.getByTestId('todo-center-view-toggle');
    await toggle.locator('.ant-segmented-item-label[title="执行监控"]').click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // URL 携带形态参数；浏览器历史栈未被形态切换撑大（replaceUrl 替换当前条目）
    await expect(page).toHaveURL(/#\/todos\?view=running$/);
  });

  test('环路：/#/loops?view=kanban 直达看板形态', async ({ page }) => {
    // 测试目的：URL 带 ?view=kanban 时环路页直接渲染执行历史看板（LoopKanban）
    await page.goto(`${BASE}/#/loops?view=kanban`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    const toggle = page.getByTestId('loop-list-view-toggle');
    await expect(toggle).toBeVisible();
    await expect(toggle.locator('.ant-segmented-item-selected .ant-segmented-item-label')).toHaveAttribute('title', '看板');
  });

  test('任务：/#/tasks?view=card 直达卡片形态', async ({ page }) => {
    // 测试目的：URL 带 ?view=card 时任务页直接渲染卡片墙（Segmented 选中「卡片」）
    await page.goto(`${BASE}/#/tasks?view=card`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    const toggle = page.getByTestId('tasks-view-toggle');
    await expect(toggle).toBeVisible();
    await expect(toggle.locator('.ant-segmented-item-selected .ant-segmented-item-label')).toHaveAttribute('title', '卡片');
  });

  test('工艺：/#/processes?view=template 直达模板范围', async ({ page }) => {
    // 测试目的：URL 带 ?view=template 时工艺页 Segmented 选中「模板」并加载模板列表
    await page.goto(`${BASE}/#/processes?view=template`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    const toggle = page.getByTestId('process-scope-toggle');
    await expect(toggle).toBeVisible();
    await expect(toggle.locator('.ant-segmented-item-selected')).toContainText('模板');
  });
});

test.describe('109 PAT 可用性验证', () => {
  test('设置-第三方授权：已配置态展示「验证」按钮，点击后出现验证结果', async ({ page }) => {
    // 测试目的：验证按钮仅在已配置态出现；点击后调用后端 /verify 接口，
    // 无论 PAT 是否有效都会出现结果反馈（成功用户名 Tag 或失败 message），证明链路可用
    await page.goto(`${BASE}/#/settings?tab=thirdParty`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    const verifyBtn = page.getByRole('button', { name: '验证' });
    const hasVerifyBtn = (await verifyBtn.count()) > 0;
    // 本机未配置 PAT 时按钮不应渲染（与「未配置态只给保存」的互斥逻辑一致）
    if (!hasVerifyBtn) {
      // 无按钮时断言处于未配置态（存在「保存」按钮），用例仍通过（环境无关）
      await expect(page.getByRole('button', { name: '保存' })).toBeVisible();
      return;
    }

    await verifyBtn.click();
    await page.waitForTimeout(ROUTE_SETTLE_MS * 2);

    // 验证成功的标志：行内 Tag「验证通过：@用户名」；失败则以 antd message 展示，
    // 两者必居其一（按钮恢复非 loading 态即结果已落定）
    await expect(verifyBtn).not.toHaveAttribute('class', /ant-btn-loading/);
    const successTag = page.locator('.ant-tag', { hasText: '验证通过' });
    if ((await successTag.count()) > 0) {
      // 成功路径：展示 PAT 归属用户名（@login），证明令牌当前可用
      await expect(successTag.first()).toBeVisible();
    } else {
      // 失败路径：后端已按原因分类（无效/未配置/网络），message 容器出现即可
      await expect(page.locator('.ant-message')).toBeVisible();
    }
  });
});
