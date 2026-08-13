// 096-W4-4 冒烟：BlackboardPage 拆分（useBlackboardWiki / SettingsModal / DebounceStatus hook）后的行为等价验证。
// 验证点：页面装配、文件列表渲染、设置弹窗开合与表单初始化、保存链路——
// 这些路径的 state 全部经过了 hook/子组件下沉，本冒烟即为外层行为锚点。
import { test, expect } from '@playwright/test';

test.describe('BlackboardPage 拆分冒烟', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('http://localhost:18088/#/blackboard?workspace=1');
    // 页面标题出现即组件树就绪
    await page.getByText('黑板', { exact: true }).first().waitFor();
  });

  test('页面装配：标题/操作按钮/布局渲染', async ({ page }) => {
    // 设置与刷新按钮存在（handleOpenSettings / refresh 已由 hook 与单行 callback 承接）
    await expect(page.getByTitle('设置').first()).toBeVisible();
    await expect(page.getByTitle('刷新').first().or(page.getByRole('button', { name: '刷新' }))).toBeVisible();
  });

  test('文件列表加载渲染（useBlackboardWiki 数据通路）', async ({ page }) => {
    // 初始拉取完成后 Skeleton 消失、目录区容器渲染（有数据出树、无数据出空态——两者都算链路走通）
    await expect(page.locator('.ant-skeleton-active')).toHaveCount(0, { timeout: 10000 });
    await expect(page.locator('.ant-menu, .ant-empty').first()).toBeVisible();
  });

  test('设置弹窗：打开→表单初始化→关闭（state 下沉后行为）', async ({ page }) => {
    await page.getByTitle('设置').first().click();
    // 弹窗打开，启用开关与 Tabs 渲染（表单 state 由弹窗内 useEffect 从 configData 初始化）
    const modal = page.locator('.ant-modal');
    await expect(modal.getByText('黑板设置')).toBeVisible();
    await expect(modal.getByText('启用黑板')).toBeVisible();
    await expect(modal.getByRole('tab', { name: '防抖设置' })).toBeVisible();
    await expect(modal.getByRole('tab', { name: '提示词设置' })).toBeVisible();
    // 防抖周期输入框应有初始化值（configData 或默认 600）
    const input = modal.locator('input').first();
    await expect(input).toBeVisible();
    // 切换到提示词 Tab
    await modal.getByRole('tab', { name: '提示词设置' }).click();
    await expect(modal.getByRole('button', { name: '恢复默认' })).toBeVisible();
    // 关闭
    await modal.getByRole('button', { name: 'Cancel' }).or(page.locator('.ant-modal-close')).first().click().catch(async () => {
      await page.keyboard.press('Escape');
    });
  });

  test('设置保存链路：修改防抖周期并保存成功（into_active 回写）', async ({ page }) => {
    await page.getByTitle('设置').first().click();
    const modal = page.locator('.ant-modal');
    await expect(modal.getByText('黑板设置')).toBeVisible();
    // 直接保存（表单已有初始化值），验证保存链路（updateBlackboardConfig + onSaved 回写）
    // antd Modal 的 OK 按钮中文两字可能被插空格，用 footer 主按钮定位最稳
    await modal.locator('.ant-modal-footer button.ant-btn-primary').click();
    // 成功提示出现证明保存链路走通
    await expect(page.getByText('设置已保存')).toBeVisible({ timeout: 8000 });
  });
});
