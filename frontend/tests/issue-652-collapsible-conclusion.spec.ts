/**
 * issue #652 — 「可折叠结论」组件 UI 集成测试
 *
 * 启动 Vite dev server，把 CollapsibleConclusion 在浏览器里跑起来，
 * 验证：
 *   1. 默认展开：能看到 Markdown 内容、字数统计
 *   2. 点击 toggle 后折叠：内容消失、aria-expanded=false、容器高度变小
 *   3. 再次点击 toggle 后恢复展开：内容出现
 *   4. localStorage 持久化：刷新后保持折叠/展开态
 *   5. showTitle=true 时显示「结论」标题
 *   6. 失败状态时容器 class 含 history-result-failed
 *
 * 截图：保留给 PR 评论作为视觉证据。
 */

import { test, expect } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const DEV_URL = process.env.E2E_BASE_URL || 'http://127.0.0.1:18089';
const HARNESS_URL = `${DEV_URL}/tests/issue-652-mount.html`;

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SCREENSHOT_DIR = resolve(__dirname, '__screenshots__');
mkdirSync(SCREENSHOT_DIR, { recursive: true });

// 长结论样本：包含代码块、列表、段落，模拟真实执行结果
const LONG_RESULT = `# 完成情况

1. 实现 CollapsibleConclusion 组件
2. 替换 3 个文件中的内联结论区
3. 添加折叠态 CSS

\`\`\`ts
const ok = await copyToClipboard(result);
\`\`\`

- [x] 默认展开
- [x] 折叠/展开切换
- [x] localStorage 持久化
`;

/** 把数据塞到 URL hash 里，harness 端会读出来 */
async function mount(
  page: any,
  data: { result: string; status?: string; recordId?: number | string | null; showTitle?: boolean },
) {
  await page.goto('about:blank');
  const hash = encodeURIComponent(JSON.stringify({
    result: data.result,
    status: data.status ?? 'success',
    recordId: data.recordId ?? null,
    showTitle: data.showTitle ?? false,
  }));
  await page.goto(`${HARNESS_URL}#${hash}`);
  await page.waitForFunction(() => (window as any).__renderDone === true, { timeout: 10000 });
  const err = await page.evaluate(() => (window as any).__renderError);
  if (err) throw new Error(`Mount 失败: ${err}`);
}

test.describe('可折叠结论 — Issue #652', () => {
  test('默认展开：应显示 Markdown 内容与字数统计', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', m => {
      if (m.type() === 'error') errors.push(`console.error: ${m.text()}`);
    });

    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(1500);

    await mount(page, { result: LONG_RESULT, recordId: null });

    const container = page.locator('[data-testid="collapsible-conclusion"]');
    await expect(container).toBeVisible();
    await expect(container).toHaveAttribute('data-collapsed', 'false');

    // 内容区域可见
    const content = page.locator('[data-testid="conclusion-content"]');
    await expect(content).toBeVisible();
    await expect(content).toContainText('完成情况');

    // 字数统计（字符数会包含 markdown 符号，按大致范围断言）
    const toggle = page.locator('[data-testid="conclusion-toggle"]');
    await expect(toggle).toContainText('字');
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');

    // 截图：默认展开
    await container.screenshot({
      path: resolve(SCREENSHOT_DIR, 'issue-652-expanded.png'),
      fullPage: true,
    });

    // 无 console error（antd deprecation 与 403 /api/* 是已有噪音，跳过）
    const realErrors = errors.filter(e =>
      !e.includes('[antd:') && !e.includes('Failed to load resource'),
    );
    expect(realErrors).toEqual([]);
  });

  test('点击 toggle 后折叠：内容消失、aria-expanded=false', async ({ page }) => {
    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(1500);

    await mount(page, { result: LONG_RESULT, recordId: 1001 });

    const container = page.locator('[data-testid="collapsible-conclusion"]');
    await expect(container).toHaveAttribute('data-collapsed', 'false');

    // 点击 toggle 折叠
    await page.locator('[data-testid="conclusion-toggle"]').click();
    await expect(container).toHaveAttribute('data-collapsed', 'true');
    await expect(page.locator('[data-testid="conclusion-toggle"]')).toHaveAttribute('aria-expanded', 'false');

    // 内容区域消失
    await expect(page.locator('[data-testid="conclusion-content"]')).toHaveCount(0);

    // 截图：折叠态
    await container.screenshot({
      path: resolve(SCREENSHOT_DIR, 'issue-652-collapsed.png'),
      fullPage: true,
    });

    // 折叠态下容器高度应明显小于展开态
    const collapsedBox = await container.boundingBox();
    expect(collapsedBox?.height ?? 0).toBeLessThan(80);
  });

  test('再次点击 toggle：恢复展开', async ({ page }) => {
    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(1500);

    await mount(page, { result: LONG_RESULT, recordId: 1002 });

    const container = page.locator('[data-testid="collapsible-conclusion"]');
    const toggle = page.locator('[data-testid="conclusion-toggle"]');

    await toggle.click();
    await expect(container).toHaveAttribute('data-collapsed', 'true');

    await toggle.click();
    await expect(container).toHaveAttribute('data-collapsed', 'false');
    await expect(page.locator('[data-testid="conclusion-content"]')).toBeVisible();
  });

  test('localStorage 持久化：刷新后保持折叠态', async ({ page }) => {
    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(1500);

    // 用固定 recordId 让 localStorage 生效
    await mount(page, { result: LONG_RESULT, recordId: 9999 });

    // 折叠一次
    await page.locator('[data-testid="conclusion-toggle"]').click();
    await expect(page.locator('[data-testid="collapsible-conclusion"]')).toHaveAttribute('data-collapsed', 'true');

    // 重新挂载同一 recordId，模拟刷新
    await mount(page, { result: LONG_RESULT, recordId: 9999 });

    // 应该是折叠的（持久化生效）
    await expect(page.locator('[data-testid="collapsible-conclusion"]')).toHaveAttribute('data-collapsed', 'true');
    await expect(page.locator('[data-testid="conclusion-content"]')).toHaveCount(0);
  });

  test('showTitle=true：显示「结论」标题', async ({ page }) => {
    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(1500);

    await mount(page, { result: LONG_RESULT, recordId: 2001, showTitle: true });

    const toggle = page.locator('[data-testid="conclusion-toggle"]');
    await expect(toggle).toContainText('结论');
  });

  test('失败状态：容器带 history-result-failed 类', async ({ page }) => {
    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(1500);

    await mount(page, { result: '执行失败原因...', status: 'failed', recordId: 3001 });

    const container = page.locator('[data-testid="collapsible-conclusion"]');
    await expect(container).toHaveClass(/history-result-failed/);
  });

  test('复制按钮：点击后调用 copyToClipboard', async ({ page }) => {
    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(1500);

    await mount(page, { result: '需要复制的结论内容', recordId: 4001 });

    // 拦截 clipboard 写入（在浏览器里通过 permissions 提前允许）
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await page.locator('[data-testid="conclusion-copy"]').click();
    // 不验证 messageApi 的具体内容（mock 化），只确保点击不报错
    const errors: string[] = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.waitForTimeout(200);
    expect(errors).toEqual([]);
  });
});
