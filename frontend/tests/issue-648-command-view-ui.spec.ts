/**
 * issue #648 — 命令视图 UI 集成测试
 *
 * 启动 Vite dev server，把 CommandPanel 在浏览器里跑起来，
 * 注入一组合成的 log 数据，验证组件渲染、生成截图。
 *
 * ## 实现要点
 *
 * - 用 `page.addScriptTag({ type: 'module', url: ... })` 加载一段 module
 *   脚本——这样 Vite 的 import map（/node_modules/.vite/deps/*）才生效，
 *   bare specifier 'react' 才会被解析。
 * - 直接 React.createRoot + createElement 把组件挂到测试容器。
 * - 不依赖真实后端 / 路由 / 鉴权。
 *
 * 截图：命令视图在典型日志下的视觉证据，CI 中可附在 PR description。
 */

import { test, expect } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

// 端口与 vite.config.ts server.port 对齐（5173），与 playwright.config.ts baseURL 一致。
const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:5173';
const HARNESS_URL = `${DEV_URL}/tests/issue-648-mount.html`;

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SCREENSHOT_DIR = resolve(__dirname, '__screenshots__');
mkdirSync(SCREENSHOT_DIR, { recursive: true });

/**
 * 把渲染结果以 Promise 形式暴露到 window.__rendered，方便 page.evaluate
 * 拿到结果。脚本作为 module 加载，享受 Vite 的 import map。
 */
async function mountCommandPanel(page: any, executor: string, logs: any[]): Promise<{ text: string; html: string }> {
  // 先在空白页注入数据，然后跳到 mount 页面
  await page.goto('about:blank');
  await page.evaluate((data: any) => {
    (window as any).__testLogs = data.logs;
    (window as any).__testExecutor = data.executor;
  }, { logs, executor });
  // 把数据透传到 harness：跨 page 跳需要再写一次，因为 about:blank 不共享存储
  // 简化：把数据序列化进 URL hash
  const hash = encodeURIComponent(JSON.stringify({ logs, executor }));
  await page.goto(`${HARNESS_URL}#${hash}`);

  // 等 mount 脚本完成
  await page.waitForFunction(() => (window as any).__renderDone === true, { timeout: 10000 });
  const renderError = await page.evaluate(() => (window as any).__renderError);
  if (renderError) {
    throw new Error(`Mount 失败: ${renderError}`);
  }
  return await page.evaluate(() => {
    const el = document.getElementById('test-command-panel');
    return { text: el?.textContent || '', html: el?.innerHTML || '' };
  });
}

const SAMPLE_LOGS = [
  { timestamp: '2024-01-01T10:00:00Z', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'git pull origin main' }), toolCallId: 'c1' },
  { timestamp: '2024-01-01T10:00:01Z', type: 'tool_result', content: 'Already up to date.', toolCallId: 'c1', isError: false },
  { timestamp: '2024-01-01T10:00:02Z', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'npm install' }), toolCallId: 'c2' },
  { timestamp: '2024-01-01T10:00:08Z', type: 'tool_result', content: 'added 120 packages in 6s', toolCallId: 'c2', isError: false },
  { timestamp: '2024-01-01T10:00:09Z', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'docker build .' }), toolCallId: 'c3' },
  { timestamp: '2024-01-01T10:00:13Z', type: 'tool_result', content: 'ERROR: build failed\ndockerfile:10:2 unknown instruction', toolCallId: 'c3', isError: true },
];

test.describe('命令视图 UI — Issue #648', () => {
  test('应在浏览器中渲染「命令」视图并展示命令卡片', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', m => {
      if (m.type() === 'error') errors.push(`console.error: ${m.text()}`);
    });

    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(2000);

    const rendered = await mountCommandPanel(page, 'claudecode', SAMPLE_LOGS);

    // 3 条命令都应该被提取并展示
    expect(rendered.text).toContain('git pull origin main');
    expect(rendered.text).toContain('npm install');
    expect(rendered.text).toContain('docker build .');
    // 共 N 条命令的提示
    expect(rendered.text).toContain('共 3 条命令');
    // 成功 / 失败 标签都要出现
    expect(rendered.text).toContain('成功');
    expect(rendered.text).toContain('失败');

    // 验证无 console error（antd deprecation 警告和 403 是已有噪音，跳过）
    const realErrors = errors.filter(e =>
      !e.includes('[antd:') && !e.includes('Failed to load resource'),
    );
    expect(realErrors).toEqual([]);

    // 截图
    const panel = page.locator('#test-command-panel');
    await panel.screenshot({
      path: resolve(SCREENSHOT_DIR, 'command-view.png'),
      fullPage: true,
    });
  });

  test('hermes 执行器应显示「不支持」提示', async ({ page }) => {
    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(2000);

    const result = await mountCommandPanel(page, 'hermes', []);
    expect(result.text).toContain('Hermes');
    expect(result.text).toContain('不支持');
  });

  test('空日志应显示「未捕获到」空态', async ({ page }) => {
    await page.goto(DEV_URL);
    await page.waitForSelector('#root', { state: 'attached' });
    await page.waitForTimeout(2000);

    const result = await mountCommandPanel(page, 'claudecode', []);
    expect(result.text).toContain('未捕获到');
  });
});
