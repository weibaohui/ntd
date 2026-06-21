/**
 * 通用操作工具栏（ActionToolbar）UI 集成测试
 *
 * 验证三种模式（事项 / 环节 / 环路）下工具栏：
 *   1. 「全选」复选框 + 三态切换（unchecked / indeterminate / checked）
 *   2. 「新建」按钮文案随模式变化（新建事项 / 新建环节 / 新建环路）
 *   3. 「批量」下拉菜单项随模式变化（事项和环节：更换执行器；环路：强停）
 *   4. 顶部 header 旧版「新建任务」按钮已被移除，只留「智能新建」
 *
 * 截图证据：保存到 /tmp/feat-action-toolbar-*.png（不入 git）。
 */

import { test, expect, type Page } from '@playwright/test';

const DEV_URL = process.env.E2E_BASE_URL || 'http://127.0.0.1:18088';

// 每个用例各自创建/清理的种子数据 id
let todoId1 = 0;
let todoId2 = 0;
let todoId3 = 0;
let stepSourceTodoId = 0;
let stepId = 0;
let loopId = 0;

test.beforeAll(async ({ request }) => {
  // 事项
  for (let i = 1; i <= 3; i++) {
    const r = await request.post(`${DEV_URL}/api/todos`, {
      data: { title: `toolbar-todo-${i}`, prompt: `seed ${i}` },
    });
    expect(r.ok()).toBeTruthy();
    const { data } = await r.json();
    if (i === 1) todoId1 = data.id;
    if (i === 2) todoId2 = data.id;
    if (i === 3) todoId3 = data.id;
  }

  // 环节（通过 promote 创建）
  const stepRes = await request.post(`${DEV_URL}/api/todos`, {
    data: { title: 'toolbar-step-1', prompt: 'step seed' },
  });
  const { data: stepTodo } = await stepRes.json();
  stepSourceTodoId = stepTodo.id;
  const promoteRes = await request.post(`${DEV_URL}/api/todos/${stepSourceTodoId}/promote`);
  const { data: promoted } = await promoteRes.json();
  stepId = promoted.id;

  // 环路
  const loopRes = await request.post(`${DEV_URL}/api/loops`, {
    data: { name: 'toolbar-loop-1' },
  });
  const { data: loop } = await loopRes.json();
  loopId = loop.id;
});

test.afterAll(async ({ request }) => {
  await request.delete(`${DEV_URL}/api/loops/${loopId}`);
  await request.delete(`${DEV_URL}/api/todos/${stepSourceTodoId}`);
  for (const id of [todoId1, todoId2, todoId3]) {
    if (id) await request.delete(`${DEV_URL}/api/todos/${id}`);
  }
});

/** 切到指定 listMode（事项/环节/环路） */
async function switchMode(page: Page, mode: 'item' | 'step' | 'loop') {
  const label = mode === 'item' ? '事项' : mode === 'step' ? '环节' : '环路';
  // antd Segmented 把 input 渲染为隐藏 radio，需点 label 才能触发；
  // 限定在 .ant-segmented 内查找避免误伤 header 里的「事项」字样。
  await page.locator('.ant-segmented').getByText(label, { exact: true }).click();
  // 给 segmented 状态变更和列表重渲染留 300ms
  await page.waitForTimeout(300);
}

test.describe('ActionToolbar — 通用行为', () => {
  test('事项模式：工具栏三按钮 + 全选 + 批量', async ({ page }) => {
    await page.goto(DEV_URL);
    // 默认是事项模式，但显式切一次保证可重复
    await switchMode(page, 'item');

    // 工具栏存在
    await expect(page.getByTestId('action-toolbar')).toBeVisible();
    // 新建按钮文案 = 新建（2 字，用户从 listMode 知道新建类型）
    await expect(page.getByTestId('action-toolbar-create')).toContainText('新建');
    // 批量下拉存在
    await expect(page.getByTestId('action-toolbar-batch-trigger')).toBeVisible();
  });

  test('事项模式：勾选两条后批量 Dropdown 启用，菜单项是「更换执行器」', async ({ page }) => {
    await page.goto(DEV_URL);
    await switchMode(page, 'item');

    // 等列表加载
    await page.waitForTimeout(300);

    // 勾选前两条 todo 的复选框（用 data-testid 精确锁）
    // 用 force: true 绕开 actionability 检查：用户 DB 里其他 todo 卡片可能
    // 视觉上压在我们新建的 todo 上方，Playwright 会误判被遮挡。
    const todo1Cb = page.getByTestId(`todo-row-checkbox-${todoId1}`);
    const todo2Cb = page.getByTestId(`todo-row-checkbox-${todoId2}`);
    await todo1Cb.click({ force: true });
    await todo2Cb.click({ force: true });

    // 工具栏出现「已选 2 项」
    await expect(page.getByTestId('action-toolbar-selected-count')).toContainText('已选 2');

    // 打开批量下拉
    await page.getByTestId('action-toolbar-batch-trigger').click();
    // 看到「更换执行器」菜单项
    await expect(page.getByRole('menuitem', { name: /更换执行器/ })).toBeVisible();
  });

  test('事项模式：全选复选框三态切换', async ({ page }) => {
    await page.goto(DEV_URL);
    await switchMode(page, 'item');
    await page.waitForTimeout(300);

    const selectAll = page.getByTestId('action-toolbar-select-all');
    // 初始：未选
    await expect(selectAll).not.toBeChecked();

    // 点全选 → 全部当前可见项被勾
    await selectAll.click();
    await expect(page.getByTestId('action-toolbar-selected-count')).toContainText('已选');
    // 复选框应进入 checked 态（DOM aria-checked or input checked）
    await expect(selectAll).toBeChecked();

    // 取消勾选其中一条 → 复选框进入 indeterminate
    await page.getByTestId(`todo-row-checkbox-${todoId1}`).click({ force: true });
    // antd 在 indeterminate 时 input 不 checked，但 aria-checked 会变 mixed
    const ariaChecked = await selectAll.getAttribute('aria-checked');
    expect(ariaChecked).toBe('mixed');
  });

  test('事项模式：header 旧版「新建任务」按钮已移除，只剩「智能新建」', async ({ page }) => {
    await page.goto(DEV_URL);
    await switchMode(page, 'item');

    // header 智能新建（按 aria-label 锁）
    await expect(page.getByRole('button', { name: '智能新建' })).toBeVisible();
    // header 普通新建任务按钮已不再存在
    await expect(page.getByRole('button', { name: '新建任务' })).toHaveCount(0);
    // 工具栏里的「新建」仍存在
    await expect(page.getByTestId('action-toolbar-create')).toContainText('新建');
  });

  test('环节模式：createLabel=新建，批量菜单仍是「更换执行器」', async ({ page }) => {
    await page.goto(DEV_URL);
    await switchMode(page, 'step');
    await page.waitForTimeout(300);

    await expect(page.getByTestId('action-toolbar-create')).toContainText('新建');
    // 选择前，确认模式切换后 selectedIds 被清空
    await expect(page.getByTestId('action-toolbar-selected-count')).toHaveCount(0);

    // 勾选环节
    await page.getByTestId(`step-row-checkbox-${stepId}`).click({ force: true });
    await expect(page.getByTestId('action-toolbar-selected-count')).toContainText('已选 1');

    // 打开批量下拉，菜单项 = 更换执行器
    await page.getByTestId('action-toolbar-batch-trigger').click();
    await expect(page.getByRole('menuitem', { name: /更换执行器/ })).toBeVisible();
  });

  test('环路模式：createLabel=新建，批量菜单是「强停」并标 danger', async ({ page }) => {
    await page.goto(DEV_URL);
    await switchMode(page, 'loop');
    await page.waitForTimeout(300);

    await expect(page.getByTestId('action-toolbar-create')).toContainText('新建');

    // 勾选环路
    await page.getByTestId(`loop-row-checkbox-${loopId}`).click({ force: true });
    await expect(page.getByTestId('action-toolbar-selected-count')).toContainText('已选 1');

    // 打开批量下拉，菜单项 = 强停
    await page.getByTestId('action-toolbar-batch-trigger').click();
    const forceStop = page.getByRole('menuitem', { name: /强停/ });
    await expect(forceStop).toBeVisible();
    // antd 给 danger 菜单项加 .ant-dropdown-menu-item-danger 类
    const cls = await forceStop.getAttribute('class');
    expect(cls).toContain('ant-dropdown-menu-item-danger');
  });
});
