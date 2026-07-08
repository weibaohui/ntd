import { test, expect } from '@playwright/test';

/**
 * 主题页操作工具条（生成建议 + 删除当前主题）渲染与交互验证。
 *
 * 依赖 make dev（http://localhost:18088）与 dev 工作空间下至少存在一个 topic 文件
 *（~/.ntd/workspace/1/wiki/topics/*.md）。删除走二次确认，用例只验证弹窗后取消，
 * 不真正删除用户数据。
 */
const BLACKBOARD_URL = 'http://localhost:18088/#/blackboard?workspace=1';

test.describe('黑板主题页操作工具条', () => {
  test('主题页渲染生成建议与删除按钮，点删除弹出二次确认', async ({ page }) => {
    await page.goto(BLACKBOARD_URL);
    // 等待目录加载完成：至少一个主题菜单项出现
    await expect(page.locator('.ant-menu-item').first()).toBeVisible({ timeout: 15000 });

    // 生成建议按钮（ProposalButton 文案）应出现在内容区工具条
    const proposeBtn = page.getByRole('button', { name: '生成建议' });
    await expect(proposeBtn).toBeVisible({ timeout: 10000 });

    // 删除主题按钮应出现
    const deleteBtn = page.getByRole('button', { name: /删除主题/ });
    await expect(deleteBtn).toBeVisible();

    // 点击删除应弹出二次确认 Modal，避免误删
    await deleteBtn.click();
    const confirmModal = page.locator('.ant-modal-confirm').filter({ hasText: '删除主题' });
    await expect(confirmModal).toBeVisible({ timeout: 5000 });
    // 确认框内出现「删除」「取消」两个按钮，证明确实是二次确认而非直接删除。
    // antd zhCN locale 会对两字中文按钮插入空格（删除→"删 除"），用 \s* 兼容。
    await expect(confirmModal.getByRole('button', { name: /删\s*除/ })).toBeVisible();
    await expect(confirmModal.getByRole('button', { name: /取\s*消/ })).toBeVisible();
    // 按 Esc 触发 onCancel 关闭（antd Modal.confirm 默认 keyboard=true），
    // 比精准点按钮更稳，且不会真正执行删除。
    await page.keyboard.press('Escape');
    await expect(confirmModal).toBeHidden({ timeout: 5000 });
  });

  test('非 topic 页（执行日志）不渲染主题工具条', async ({ page }) => {
    await page.goto(BLACKBOARD_URL);
    await expect(page.locator('.ant-menu-item').first()).toBeVisible({ timeout: 15000 });

    // 切到「执行日志」菜单项（log 页，非 topic）
    const logItem = page.locator('.ant-menu-item').filter({ hasText: '执行日志' });
    if (await logItem.count()) {
      await logItem.first().click();
      // 工具条不应出现：页面里不应有删除主题按钮
      await expect(page.getByRole('button', { name: /删除主题/ })).toHaveCount(0);
      await expect(page.getByRole('button', { name: '生成建议' })).toHaveCount(0);
    }
  });
});
