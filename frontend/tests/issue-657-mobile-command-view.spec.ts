/**
 * issue #657 — 窄屏命令视图渲染校验
 *
 * 复现 PR #657 修复的问题：手机端命令视图按钮可见，但点开后组件缺少
 * `viewMode === 'command'` 渲染分支；同时验证 defaultOpen 与 titleMap 重构
 * 是否正确生效。
 *
 * 流程：
 * 1) 启动 Vite dev server
 * 2) 用 iPhone 12 尺寸的 viewport 模拟窄屏
 * 3) mount NarrowLogView / ContinuationLogView / ContinuationLogsLoader，
 *    传入 viewMode=command
 * 4) 校验 CommandPanel 已挂载（命令卡片、命令计数出现）
 * 5) 校验 details 元素默认展开（defaultOpen 含 'command'）
 * 6) 截图留档
 */
import { test, expect } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

// 端口与 vite.config.ts server.port 对齐（5173），避免与 baseURL 不一致导致连接拒绝。
// 也允许通过 E2E_BASE_URL 在 CI 上覆盖。
const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:5173';
const HARNESS_URL = `${DEV_URL}/tests/issue-657-mount.html`;

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SCREENSHOT_DIR = resolve(__dirname, '__screenshots__');
mkdirSync(SCREENSHOT_DIR, { recursive: true });

interface LogEntry {
  timestamp: string;
  type: string;
  content: string;
  toolName?: string;
  toolInputJson?: string;
  toolCallId?: string;
  isError?: boolean;
}

const SAMPLE_LOGS: LogEntry[] = [
  { timestamp: '2024-01-01T10:00:00Z', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'git pull origin main' }), toolCallId: 'c1' },
  { timestamp: '2024-01-01T10:00:01Z', type: 'tool_result', content: 'Already up to date.', toolCallId: 'c1', isError: false },
  { timestamp: '2024-01-01T10:00:02Z', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'npm install' }), toolCallId: 'c2' },
  { timestamp: '2024-01-01T10:00:08Z', type: 'tool_result', content: 'added 120 packages in 6s', toolCallId: 'c2', isError: false },
];

/** 把数据序列化进 URL hash，跳到 harness 页面，等 mount 完成。 */
async function mountAndRead(page: any, payload: object) {
  const hash = encodeURIComponent(JSON.stringify(payload));
  await page.goto(`${HARNESS_URL}#${hash}`);
  await page.waitForFunction(() => (window as any).__renderDone === true, { timeout: 10000 });
  const renderError = await page.evaluate(() => (window as any).__renderError);
  if (renderError) {
    throw new Error(`Mount 失败: ${renderError}`);
  }
  return await page.evaluate(() => {
    const el = document.getElementById('test-target');
    return { text: el?.textContent || '', html: el?.innerHTML || '' };
  });
}

test.describe('issue #657 — 窄屏命令视图', () => {
  // iPhone 12 viewport 模拟窄屏
  test.use({ viewport: { width: 390, height: 844 } });

  test('NarrowLogView 在 command 视图下应渲染 CommandPanel', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', m => {
      if (m.type() === 'error') errors.push(`console.error: ${m.text()}`);
    });

    const rendered = await mountAndRead(page, {
      component: 'NarrowLogView',
      logs: SAMPLE_LOGS,
      executor: 'claudecode',
      viewMode: 'command',
      recordId: 42,
    });

    // 命令卡片应当出现
    expect(rendered.text).toContain('git pull origin main');
    expect(rendered.text).toContain('npm install');
    // 命令计数
    expect(rendered.text).toContain('共 2 条命令');
    // defaultOpen 应当展开 details
    const isOpen = await page.evaluate(() => {
      const d = document.querySelector('#test-target details');
      return d ? d.hasAttribute('open') : false;
    });
    expect(isOpen).toBe(true);
    // title 应当是「命令视图 (4 条)」（displayLogs.length === 4）
    expect(rendered.text).toContain('命令视图');

    // 截图
    const target = page.locator('#test-target');
    await target.screenshot({
      path: resolve(SCREENSHOT_DIR, 'narrow-log-view-command.png'),
      fullPage: true,
    });

    const realErrors = errors.filter(e =>
      !e.includes('[antd:') && !e.includes('Failed to load resource'),
    );
    expect(realErrors).toEqual([]);
  });

  test('ContinuationLogView 在 command 视图下应渲染 CommandPanel', async ({ page }) => {
    const rendered = await mountAndRead(page, {
      component: 'ContinuationLogView',
      logs: SAMPLE_LOGS,
      executor: 'claudecode',
      viewMode: 'command',
      recordId: 7,
    });

    // 应当出现命令视图面板
    expect(rendered.text).toContain('git pull origin main');
    expect(rendered.text).toContain('共 2 条命令');
    // 标题
    expect(rendered.text).toContain('命令 (4)');
  });

  test('ContinuationLogsLoader 在 command 视图下应渲染 CommandPanel', async ({ page }) => {
    // 通过 mount 显式传入 logs 跳过懒加载，让组件在静态 harness 下也能渲染命令视图。
    const rendered = await mountAndRead(page, {
      component: 'ContinuationLogsLoader',
      logs: SAMPLE_LOGS,
      executor: 'claudecode',
      viewMode: 'command',
      recordId: 9,
    });

    // 标题应是「命令 (N)」
    expect(rendered.text).toMatch(/命令 \(\d+\)/);
    // Segmented 三个选项都应渲染：日志/对话/命令
    expect(rendered.text).toContain('日志');
    expect(rendered.text).toContain('对话');
    expect(rendered.text).toContain('命令');
    // CommandPanel 拿到 logs 后正常提取命令：覆盖命令卡片与计数
    expect(rendered.text).toContain('git pull origin main');
    expect(rendered.text).toContain('共 2 条命令');
  });

  test('NarrowLogView 在 log 视图下应渲染原始日志，不渲染 CommandPanel', async ({ page }) => {
    const rendered = await mountAndRead(page, {
      component: 'NarrowLogView',
      logs: SAMPLE_LOGS,
      executor: 'claudecode',
      viewMode: 'log',
      recordId: 42,
    });

    // 标题应是「查看日志 (4 条)」
    expect(rendered.text).toContain('查看日志');
    // 不应出现 CommandPanel 计数
    expect(rendered.text).not.toContain('共 2 条命令');
  });

  // PR #657 复查 C1 回归测试：useState 初始值只读一次。
  // 之前从 log 视图切到 command 视图时 details 不会自动展开，用户必须再点一次 summary。
  // 修复后：切到 chat/command 应自动展开 details 露出 CommandPanel。
  test('NarrowLogView 在 log 视图下点击「命令」应自动展开 details', async ({ page }) => {
    const rendered = await mountAndRead(page, {
      component: 'NarrowLogView',
      logs: SAMPLE_LOGS,
      executor: 'claudecode',
      viewMode: 'log',
      recordId: 42,
    });

    // 初始 log 视图：标题是「查看日志」，details 默认收起。
    expect(rendered.text).toContain('查看日志');
    const initiallyOpen = await page.evaluate(() => {
      const d = document.querySelector('#test-target details');
      return d ? d.hasAttribute('open') : false;
    });
    expect(initiallyOpen).toBe(false);

    // 模拟用户在 Segmented 上点「命令」按钮：useEffect 应自动展开 details。
    await page.locator('#test-target .ant-segmented-item:has-text("命令")').first().click();
    await page.waitForTimeout(300);

    const afterOpen = await page.evaluate(() => {
      const d = document.querySelector('#test-target details');
      return d ? d.hasAttribute('open') : false;
    });
    expect(afterOpen).toBe(true);

    // CommandPanel 内容应可见
    const cmdText = await page.evaluate(() => {
      const d = document.querySelector('#test-target');
      return d?.textContent || '';
    });
    expect(cmdText).toContain('git pull origin main');
    expect(cmdText).toContain('共 2 条命令');
  });
});
