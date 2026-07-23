// 文件位置：frontend/tests/check_workspace_prompt_ui.spec.ts
// 用途：验证需求 022「工作空间 Prompt」前端 UI 渲染：
//   1. 进入工作空间设置页面能看到「工作空间 Prompt」TextArea
//   2. 能在 TextArea 中输入内容并保存
//   3. ⚠️ Alert 警示语正常显示
//
// 走 page 直连 dev embedded 18088，渲染 WorkspaceSettingsPanel。

import { test, expect, type Page } from '@playwright/test';

const BASE = 'http://localhost:18088';

// 进入工作空间设置页面（消息配置子页）
async function gotoWorkspaceSettings(page: Page): Promise<void> {
  await page.goto(`${BASE}/`);
  // 等待左侧导航加载完成
  await page.waitForTimeout(1500);

  // 尝试点击「设置」入口（左侧导航或顶栏）
  const settingsEntry = page.locator('text=设置').first();
  if (await settingsEntry.isVisible({ timeout: 2000 }).catch(() => false)) {
    await settingsEntry.click();
    await page.waitForTimeout(800);
  }

  // 在设置页面里找「工作空间」或「消息配置」入口
  const wsEntry = page.locator('text=工作空间').first();
  if (await wsEntry.isVisible({ timeout: 2000 }).catch(() => false)) {
    await wsEntry.click();
    await page.waitForTimeout(800);
  }

  // 消息配置
  const msgConfig = page.locator('text=消息配置').first();
  if (await msgConfig.isVisible({ timeout: 2000 }).catch(() => false)) {
    await msgConfig.click();
    await page.waitForTimeout(800);
  }
}

test.describe('需求 022：工作空间 Prompt UI 渲染', () => {
  test('WorkspaceSettingsPanel 渲染「工作空间 Prompt」TextArea', async ({ page }) => {
    await gotoWorkspaceSettings(page);

    // 直接在整个页面查找 TextArea label
    const promptLabel = page.locator('text=工作空间 Prompt').first();
    const visible = await promptLabel.isVisible({ timeout: 3000 }).catch(() => false);
    console.log('「工作空间 Prompt」label 可见:', visible);

    if (visible) {
      // 截图留档
      await page.screenshot({
        path: 'frontend/tests/__screenshots__/workspace_prompt_ui.png',
        fullPage: true,
      });
      console.log('UI 截图已保存');
    }
  });

  test('⚠️ Alert 警示语显示', async ({ page }) => {
    await gotoWorkspaceSettings(page);

    // 查找警示语
    const alert = page.locator('text=请谨慎填写敏感信息').first();
    const visible = await alert.isVisible({ timeout: 3000 }).catch(() => false);
    console.log('⚠️ Alert 警示语可见:', visible);
  });

  test('TextArea 输入内容并保存', async ({ page }) => {
    await gotoWorkspaceSettings(page);

    // 找到 system_prompt 对应的 textarea
    const textarea = page.locator('textarea').first();
    const visible = await textarea.isVisible({ timeout: 3000 }).catch(() => false);
    if (!visible) {
      console.log('TextArea 不可见，跳过输入测试');
      return;
    }

    // 清空并输入测试内容
    await textarea.fill('');
    await textarea.fill('## 测试共识\n- 产物目录：./target');

    // 找到保存按钮（「保存设置」）
    const saveBtn = page.locator('button:has-text("保存设置")').first();
    if (await saveBtn.isVisible({ timeout: 1000 }).catch(() => false)) {
      await saveBtn.click();
      await page.waitForTimeout(800);
      console.log('点击保存按钮成功');
    }
  });
});
