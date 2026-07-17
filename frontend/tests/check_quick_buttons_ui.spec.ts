import { test, expect, type APIRequestContext } from '@playwright/test';

const BASE = 'http://localhost:18088';
const NAME = 'PW_UI测试按钮';

// 可继续会话的执行器集合（与 src/utils/executors.tsx 的 RESUMABLE_EXECUTORS 保持一致）。
// 测试里不便直接 import 那个 tsx（含 JSX/运行时代码），故在此维护一份只读副本。
const RESUMABLE_EXECUTORS = new Set([
  'claudecode', 'opencode', 'mobilecoder', 'hermes', 'kimi',
  'codewhale', 'pi', 'mimo', 'zhanlu', 'kilo',
]);

interface QB {
  id: number;
  button_name: string;
  prompt_text: string;
}

// 仅取判断可恢复所需字段，避免拉入完整 ExecutionRecord 的几十个字段
interface ResumeRecord {
  id: number;
  todo_id: number;
  status: string;
  session_id?: string | null;
  executor?: string | null;
}

async function listButtons(request: APIRequestContext): Promise<QB[]> {
  return ((await (await request.get(`${BASE}/api/quick-buttons`)).json()).data) as QB[];
}

async function deleteByName(request: APIRequestContext, name: string) {
  for (const b of await listButtons(request)) {
    if (b.button_name === name) await request.delete(`${BASE}/api/quick-buttons/${b.id}`);
  }
}

// 动态找一个可 resume 的执行记录，拼出帖子页 URL；找不到返回 null（调用方 test.skip）。
// 不写死 todo/record ID——干净库或数据变动时优雅跳过，而非在 goto 处硬失败。
async function findResumablePostUrl(request: APIRequestContext): Promise<string | null> {
  const res = await request.get(`${BASE}/api/execution-records`, {
    params: { status: 'success', limit: 50 },
  });
  if (!res.ok()) return null;
  const records = ((await res.json()).data?.records ?? []) as ResumeRecord[];
  for (const r of records) {
    if (r.session_id && r.executor && RESUMABLE_EXECUTORS.has(r.executor.toLowerCase())) {
      return `${BASE}/#/items?panel=post&id=${r.todo_id}&record=${r.id}`;
    }
  }
  return null;
}

// 驱动真实浏览器走完核心 UI 交互：加号 → 新建 → 点按钮把话术填入回复输入框。
test.describe('快捷话术按钮 UI', () => {
  test.beforeEach(async ({ request }) => {
    await deleteByName(request, NAME);
  });

  // afterEach 兜底：用例中途失败（断言前抛错）也清理，避免污染全局 quick_buttons 表
  test.afterEach(async ({ request }) => {
    await deleteByName(request, NAME);
  });

  test('加号 → 新建 → 点按钮填入回复输入框', async ({ page, request }) => {
    const url = await findResumablePostUrl(request);
    test.skip(!url, '无可 resume 的执行记录，跳过（需 dev 库有 success + resumable executor 的会话）');
    await page.goto(url!);

    // 先确认 ReplyInput 渲染：回复输入框可见 = 帖子页加载完成且该会话可 resume
    const replyInput = page.getByPlaceholder('输入回复内容...');
    await replyInput.waitFor({ state: 'visible', timeout: 20000 });

    // 快捷按钮条的「+」：dashed 且含 plus 图标，避免误选页面其他 dashed 按钮
    const plus = page.locator('button.ant-btn-dashed:has(.anticon-plus)');
    await plus.waitFor({ state: 'visible', timeout: 10000 });
    await page.screenshot({ path: 'tests/__screenshots__/qb_post.png' });

    // 打开管理弹窗
    await plus.click();
    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible({ timeout: 10000 });
    await page.getByPlaceholder('如：提取skill').fill(NAME);
    await page.getByPlaceholder(/请把刚才这次执行/).fill('UI填入测试话术');
    // 提交按钮是 Modal 内唯一 primary 按钮；antd6 文本被 span 包裹，按 role name 取不稳定
    await modal.locator('button.ant-btn-primary').click();
    await expect(modal.getByText(NAME)).toBeVisible();
    await page.screenshot({ path: 'tests/__screenshots__/qb_modal.png' });

    // 关闭弹窗，按钮条出现新建的按钮
    await page.locator('.ant-modal-close').last().click();
    await expect(modal).toHaveCount(0);

    // 点按钮条新按钮 → 回复输入框被填入话术（覆盖语义）
    await page.getByRole('button', { name: NAME }).click();
    await expect(replyInput).toHaveValue('UI填入测试话术');
    await page.screenshot({ path: 'tests/__screenshots__/qb_filled.png' });
  });
});

// 移动端：验证快捷按钮渲染 + 管理 Modal 在窄屏不溢出视口
test('移动端窄屏渲染 + Modal 不溢出', async ({ page, request }) => {
  const url = await findResumablePostUrl(request);
  test.skip(!url, '无可 resume 的执行记录，跳过移动端 UI 测试');
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto(url!);
  // 回复输入框可见 = ReplyInput（含快捷按钮条）已渲染
  await page.getByPlaceholder('输入回复内容...').waitFor({ state: 'visible', timeout: 20000 });
  await page.locator('button.ant-btn-dashed:has(.anticon-plus)').click();
  const modal = page.getByRole('dialog');
  await modal.waitFor({ state: 'visible', timeout: 10000 });
  const box = await modal.boundingBox();
  expect(box, 'Modal boundingBox 应存在').toBeTruthy();
  // 横向不溢出 375 宽视口（留 1px 容差）
  expect(box!.x, 'Modal 左侧不超出视口').toBeGreaterThanOrEqual(-1);
  expect(box!.x + box!.width, 'Modal 右侧不超出视口').toBeLessThanOrEqual(376);
  await page.screenshot({ path: 'tests/__screenshots__/qb_mobile.png', fullPage: true });
});
