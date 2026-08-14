// 资源贡献扩展 UI 验证（需求 027：事项模板 / 工艺 / 技能分享到官方仓库）。
// 覆盖：
// 1. 事项模板：用户行有分享、系统行无分享；点击用户行分享（桩 PAT）→ 后端导出 YAML →
//    Drawer 提示词含 ~/.ntd/contribution-export/todos/ 与 todos/ 远端路径。
// 2. 工艺：用户行（is_system=false，排序在前）有分享；系统行无分享。
// 3. 技能：全量可分享（不区分来源），每行都有分享按钮。
// 说明：实际提交 PR 依赖真实 GitCode 账号与 AI 执行器，无法自动化；
// 本用例只验证入口渲染与提示词内容，不触发执行。

import { test, expect } from '@playwright/test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

/** 桩 PAT：与 026 用例同款，任意非空字符串即可让 configured=true */
const STUB_PAT_PATH = path.join(os.homedir(), '.ntd', 'contribution_pat.json');
/** 原文件的备份路径：存在即代表测试前已有真实 PAT，afterAll 要原样还原 */
const STUB_PAT_BACKUP = `${STUB_PAT_PATH}.playwright-027-bak`;

/** 写入桩 PAT 使 auth/status 返回 configured=true；先备份既有文件以免覆盖用户真实凭据。 */
function installStubPat(): void {
  if (fs.existsSync(STUB_PAT_PATH)) {
    fs.copyFileSync(STUB_PAT_PATH, STUB_PAT_BACKUP);
  }
  fs.mkdirSync(path.dirname(STUB_PAT_PATH), { recursive: true });
  fs.writeFileSync(STUB_PAT_PATH, JSON.stringify({ pat: 'playwright-027-stub' }), {
    mode: 0o600,
  });
}

/** 恢复测试前的状态：有备份则还原原文件，无备份则删除桩文件回到未配置态。 */
function restorePat(): void {
  if (fs.existsSync(STUB_PAT_BACKUP)) {
    fs.copyFileSync(STUB_PAT_BACKUP, STUB_PAT_PATH);
    fs.rmSync(STUB_PAT_BACKUP);
  } else if (fs.existsSync(STUB_PAT_PATH)) {
    fs.rmSync(STUB_PAT_PATH);
  }
}

test.describe('资源分享入口（桩 PAT）', () => {
  test.beforeAll(() => installStubPat());
  test.afterAll(() => restorePat());

  test('事项模板：用户行有分享、系统行无分享；分享提示词含导出文件与 todos/ 远端路径', async ({ page }) => {
    // 进入设置-模板管理（模板管理 Tab 在设置页内）。
    await page.goto('/#/settings?tab=templates');
    await expect(page.locator('.ntd-templates-panel')).toBeVisible({ timeout: 10000 });

    // 切到事项模板子 Tab。
    await page.locator('.ntd-templates-panel .ant-tabs-tab').filter({ hasText: '事项模板' }).click();
    const todoTab = page.locator('.todo-templates-tab');
    await expect(todoTab).toBeVisible({ timeout: 10000 });

    // 新建一个用户模板（避免依赖开发库中是否有用户模板）：
    // 分类 Select 默认已是 general，只填标题与 prompt 即可保存。
    // 标题带时间戳保证唯一：即使上次运行失败残留，也不会与本次新建的行冲突（多行匹配会 strict 报错）
    const testTitle = `pw-contribute-${Date.now()}`;
    await todoTab.getByRole('button', { name: /新建模板/ }).click();
    await page.getByPlaceholder('模板标题').fill(testTitle);
    await page.getByPlaceholder('模板的 AI prompt 内容').fill('playwright 分享测试 prompt');
    // antd 两个汉字的按钮会自动插入空格（渲染为「确 定」），用正则容忍空白匹配
    await page.locator('.ant-modal-footer').getByRole('button', { name: /确\s*定/ }).click();

    // 用户行（标题匹配）应有「分享」按钮。
    // 用 tr.ant-table-row 排除 antd 的隐藏测量行（height 0），只定位真实数据行。
    const userRow = todoTab.locator('tbody tr.ant-table-row').filter({ hasText: testTitle });
    await expect(userRow.getByRole('button', { name: '分享' })).toBeVisible({ timeout: 10000 });

    // 系统模板行（含「系统」Tag）不应有「分享」按钮。
    const systemRow = todoTab.locator('tbody tr.ant-table-row').filter({ has: todoTab.getByText('系统', { exact: true }) }).first();
    await expect(systemRow.getByRole('button', { name: '分享' })).toHaveCount(0);

    // 点「分享」：先触发 onPrepare（后端导出 YAML），再查 PAT 配置态（桩 PAT → configured），
    // 组件从普通「分享」按钮切换到 ActionButton 分支。先注册响应等待再点击，避免竞态。
    const exportResp = page.waitForResponse((r) => r.url().includes('/todo-templates/') && r.url().includes('/export'));
    const statusResp = page.waitForResponse((r) => r.url().includes('/contribution/auth/status'));
    await userRow.getByRole('button', { name: '分享' }).click();
    await exportResp;
    await statusResp;
    // 等 React 完成分支切换渲染后再点第二次。
    await page.waitForTimeout(300);

    // 第二次点击 ActionButton 的「分享」：打开提交 Drawer。
    await userRow.getByRole('button', { name: '分享' }).click();

    // Drawer 中 prompt 编辑区是 textarea，断言提示词包含导出文件路径与 todos/ 远端路径。
    const promptBox = page.locator('.ant-drawer textarea');
    await promptBox.waitFor({ state: 'visible', timeout: 8000 });
    const promptText = await promptBox.inputValue();
    expect(promptText).toContain('~/.ntd/contribution-export/todos/');
    expect(promptText).toContain('todos/');
    expect(promptText).toContain('pw-contribute-');

    // 清理：关闭 Drawer（Escape 对 ActionButton 的 Drawer 不生效，点右上角关闭按钮）
    // 并等遮罩消失，否则删除按钮会被 Drawer 遮罩拦截点击。
    // 删除按钮只有图标无文字，用 anticon-delete 定位按钮；Popconfirm 确认按钮限定在 popover 内，
    // 避免误匹配页面上其它「确 定」按钮。
    await page.locator('.ant-drawer-close').click();
    await page.locator('.ant-drawer').waitFor({ state: 'hidden', timeout: 5000 });
    await userRow.locator('button').filter({ has: page.locator('.anticon-delete') }).click();
    await page.locator('.ant-popover').getByRole('button', { name: /确\s*定/ }).click();
    await expect(userRow).toHaveCount(0, { timeout: 8000 });
  });

  test('工艺：用户行有分享、系统行无分享', async ({ page }) => {
    await page.goto('/#/settings?tab=templates');
    await expect(page.locator('.ntd-templates-panel')).toBeVisible({ timeout: 10000 });
    await page.locator('.ntd-templates-panel .ant-tabs-tab').filter({ hasText: '工艺模板' }).click();
    const processTab = page.locator('.process-templates-tab');
    await expect(processTab).toBeVisible({ timeout: 10000 });

    // 排序约定：用户工艺（is_system=false）在前、系统工艺在后（ProcessTemplatesTab 排序逻辑），
    // 因此第一行应有分享、最后一行（系统）无分享。
    // 用 tr.ant-table-row 排除 antd 隐藏测量行，first/last 才是真实数据行。
    const rows = processTab.locator('tbody tr.ant-table-row');
    await expect(rows.first().getByRole('button', { name: '分享' })).toBeVisible({ timeout: 10000 });
    await expect(rows.last().getByRole('button', { name: '分享' })).toHaveCount(0);
  });

  test('技能：全量可分享，每行都有分享按钮', async ({ page }) => {
    await page.goto('/#/settings?tab=templates');
    await expect(page.locator('.ntd-templates-panel')).toBeVisible({ timeout: 10000 });
    await page.locator('.ntd-templates-panel .ant-tabs-tab').filter({ hasText: 'Skill 模板' }).click();
    const skillTab = page.locator('.skill-templates-tab');
    await expect(skillTab).toBeVisible({ timeout: 10000 });

    // 技能不做来源守卫：表格第一行即应有分享按钮（数据加载后）。
    // 用 tr.ant-table-row 排除 antd 隐藏测量行（height 0，无按钮）。
    const firstRow = skillTab.locator('tbody tr.ant-table-row').first();
    await expect(firstRow.getByRole('button', { name: '分享' })).toBeVisible({ timeout: 15000 });
  });
});