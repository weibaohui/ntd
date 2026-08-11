// 096-W1-PR4 冒烟：BackupPanel 三域收口（useBackupDomain + download util）后的行为等价验证。
// 验证点：三个子 tab 渲染各自域内容、tab 切换与 URL 深链同步正常——
// 收口前这些交互由三份逐字同构的 state/handler 驱动，收口后由三个 useBackupDomain 实例驱动，
// 本冒烟即为「行为未变」的外层锚点。
import { test, expect } from '@playwright/test';

test.describe('BackupPanel 三域收口冒烟', () => {
  test.beforeEach(async ({ page }) => {
    // 直达备份面板深链：设置页 → 备份 tab → 默认「事项备份」子 tab
    await page.goto('http://localhost:18088/#/settings?tab=backup');
    // 等 antd Tabs 挂载完成（tab 标签出现即组件树就绪）
    await page.getByRole('tab', { name: '事项备份' }).waitFor();
  });

  test('默认渲染事项备份域，可见导出/导入与自动备份配置区', async ({ page }) => {
    // 事项备份域独有：YAML 导出/导入区块
    await expect(page.getByText('事项自动备份')).toBeVisible();
    await expect(page.getByText('导出备份')).toBeVisible();
    await expect(page.getByText('导入备份')).toBeVisible();
    // 「立即备份」按钮存在（触发 handler 由 hook 提供，存在即可见——不点击以避免真实写备份文件）
    await expect(page.getByRole('button', { name: '立即备份' }).first()).toBeVisible();
  });

  test('切换到 Skill 备份域并同步 URL 深链', async ({ page }) => {
    await page.getByRole('tab', { name: 'Skill备份' }).click();
    // Skill 域独有：执行器 Skills 概览区块
    await expect(page.getByText('执行器 Skills 概览')).toBeVisible();
    await expect(page.getByText('备份各执行器下的 skills 文件夹')).toBeVisible();
    // 深链同步是收口前 handleBackupSubChange 的行为，需保持不变
    await expect(page).toHaveURL(/sub=skill-backup/);
  });

  test('切换到数据库备份域，可见附加操作（优化/日志清理）', async ({ page }) => {
    await page.getByRole('tab', { name: '数据库备份' }).click();
    // 数据库域独有：优化与日志清理区块（这两组 handler 复用 domain 的 runWithLoading）。
    // 「数据库备份」同时是 tab 标签文本，用卡片标题类限定到内容区，避免 strict mode 冲突。
    await expect(page.locator('.ant-card-head-title', { hasText: '数据库备份' })).toBeVisible();
    await expect(page.getByText('压缩优化')).toBeVisible();
    await expect(page.getByText('清理日志')).toBeVisible();
    await expect(page.getByRole('button', { name: '下载数据库' })).toBeVisible();
    await expect(page).toHaveURL(/sub=database/);
  });

  test('三域往返切换后回到事项备份，深链与内容复位', async ({ page }) => {
    // 往返一次覆盖「域状态互不影响」（收口前是三组独立 useState，收口后是三个 hook 实例）
    await page.getByRole('tab', { name: 'Skill备份' }).click();
    await page.getByRole('tab', { name: '数据库备份' }).click();
    await page.getByRole('tab', { name: '事项备份' }).click();
    await expect(page.getByText('事项自动备份')).toBeVisible();
    await expect(page).toHaveURL(/sub=todo/);
  });
});
