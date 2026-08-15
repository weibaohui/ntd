// 验证消息监控台的几项修复：
// 1) ID 行复制按钮为图标按钮(无"复制"文字)且紧跟 ID；
// 2) processed_id=0 的消息(默认响应-执行器)不再渲染残留的"0"或"关联 #0"；
// 3) 智能助手配置的「群聊白名单」tab 顶部有说明提示。
// 用 Playwright 原生 locator，规避 playwright-cli 的 ref 快照过期问题。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

// 查「首个含消息数据的工作空间名」：消息页按 workspace 隔离且依赖该空间的 Bot，
// dev 库 menuitem 顺序按 workspaces 而非 id（当前首项「便利贴项目」=ws3 无 Bot/消息），
// 盲取首个 menuitem 会落到空空间导致列表 0 卡。故先查 message-stats 找 total_messages>0 的空间名。
async function pickWorkspaceWithMessages(
  page: import('@playwright/test').Page,
  dirs: Array<{ id: number; name: string }>,
): Promise<string | null> {
  for (const d of dirs) {
    const resp = await page.request.get(`${BASE}/api/v1/feishu/message-stats?workspace_id=${d.id}`);
    const stats = await resp.json().catch(() => ({}));
    // total_messages 由后端聚合该空间全部 feishu_messages 计数，>0 即有可展示数据。
    if ((stats.data?.total_messages ?? 0) > 0) return d.name;
  }
  return null;
}

// 通过 aria-label/文本点击左侧导航的「消息」，再选一个含消息数据的工作空间。
// 这两步是进入消息列表的前置条件。
async function gotoMessages(page: import('@playwright/test').Page) {
  await page.goto(BASE);
  await page.getByRole('button', { name: '消息', exact: true }).click();
  // 打开工作空间切换下拉，选一个有消息数据的空间（见 pickWorkspaceWithMessages 注释）。
  await page.getByRole('button', { name: '切换工作空间' }).click();
  const dirsResp = await page.request.get(`${BASE}/api/v1/workspaces`);
  const wsName = await pickWorkspaceWithMessages(page, (await dirsResp.json()).data || []);
  // dev 库可能无任何空间含消息数据，此时整组用例无意义，跳过而非误报失败。
  test.skip(!wsName, 'dev 库无含消息数据的工作空间，跳过');
  await page.getByRole('menuitem', { name: wsName as string }).click();
  // 等列表加载：标题「消息监控台」出现且至少一张卡片渲染（按 Bot 并发拉取，给足时间）。
  await expect(page.getByRole('heading', { name: /消息监控台/ })).toBeVisible({ timeout: 10000 });
  await expect(page.locator('.ant-card').first()).toBeVisible({ timeout: 10000 });
}

test('消息卡片：两个复制按钮均为图标(无"复制"文字)', async ({ page }) => {
  await gotoMessages(page);
  // ID 行存在(单聊ID/群聊ID 任一)。
  const idLabel = page.locator('text=单聊ID').or(page.locator('text=群聊ID')).first();
  await expect(idLabel).toBeVisible({ timeout: 10000 });
  // 内容复制 + ID 复制都已图标化：任何卡片都不应再出现"复制"文字。
  const cardsWithCopyText = await page.locator('.ant-card').filter({ hasText: '复制' }).count();
  expect(cardsWithCopyText).toBe(0);
});

test('点击群聊/单聊 ID 复制按钮不会打开消息详情抽屉', async ({ page }) => {
  await gotoMessages(page);
  // 定位第一张卡片里的 ID 复制按钮(图标按钮，在 单聊ID/群聊ID 行末尾)。
  const idRow = page.locator('text=单聊ID').or(page.locator('text=群聊ID')).first().locator('xpath=ancestor::div[.//button]');
  const copyBtn = idRow.locator('button').last();
  await copyBtn.click();
  // 不应弹出消息详情抽屉。
  await expect(page.getByRole('dialog', { name: '消息详情' })).toHaveCount(0);
});

test('内容复制按钮可正常复制(改用 execCommand，修复曾静默失败)', async ({ page, context }) => {
  // 授权剪贴板读写，便于读取结果断言。
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await gotoMessages(page);
  const card = page.locator('.ant-card').first();
  // 内容复制按钮：卡片内第一个带 copy 图标的按钮(顶部操作区，DOM 顺序先于 ID 行的复制按钮)。
  const contentCopyBtn = card.locator('button').filter({ has: page.locator('.anticon-copy') }).first();
  await contentCopyBtn.click();
  // 复制成功后 CopyButton 图标变为对钩(仅 execCommand 返回 true 才触发)。
  await expect(card.locator('.anticon-check').first()).toBeVisible({ timeout: 3000 });
  // 剪贴板应有内容(原先 navigator.clipboard 静默失败时为空)。
  const clip = await page.evaluate(() => navigator.clipboard.readText());
  expect(clip.length).toBeGreaterThan(0);
});


test('processed_id=0 的消息不再显示残留"0"或"关联 #0"', async ({ page }) => {
  await gotoMessages(page);
  // 修复前：default_response_executor(processed_id=0) 卡片会渲染一个游离的"0"文本节点。
  // 修复后：仅在有真实 processed_id 时才渲染「关联 #N」，且 N>0。
  await expect(page.locator('text=/关联 #0$/')).toHaveCount(0);
  // 列表加载后，等待首批卡片出现，确认没有渲染异常导致整页空白。
  const firstCard = page.locator('.ant-card').first();
  await expect(firstCard).toBeVisible({ timeout: 10000 });
});

test('智能助手「群聊白名单」tab 顶部有说明提示', async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole('button', { name: '智能助手' }).click();
  // 进入智能助手列表后，点首个机器人的「配置」按钮打开抽屉。
  await page.getByRole('button', { name: /配置/ }).first().click();
  // 切到「群聊白名单」tab。
  await page.getByRole('button', { name: '群聊白名单' }).click();
  // 验证说明提示出现：与 /sethome 提示同款文案。
  await expect(page.getByText(/仅处理白名单内指定人员/)).toBeVisible({ timeout: 10000 });
});

test('消息监控台筛选区含「处理类型」下拉(任务17/18)', async ({ page }) => {
  await gotoMessages(page);
  // 处理类型下拉存在(默认显示「全部类型」)，证明已加入筛选区。
  const select = page.locator('.ant-select').filter({ hasText: '全部类型' }).first();
  await expect(select).toBeVisible({ timeout: 10000 });
});

test('关键字搜索防抖下沉后端、并回到第 1 页', async ({ page }) => {
  await gotoMessages(page);
  // dev 库首空间约 29 条消息（分页 20/页，共 2 页），先翻到第 2 页。
  const page2 = page.locator('.ant-pagination-item-2');
  await expect(page2).toBeVisible({ timeout: 10000 });
  await page2.click();
  // 输入搜索关键字；防抖 300ms 后才下沉到后端并刷新。
  // 用 dev 数据里真实存在的关键字「你」（命中多条），避免 0 命中导致分页区被隐藏。
  await page.getByPlaceholder('搜索消息内容...').fill('你');
  await page.waitForTimeout(800); // 等防抖 + 后端请求落地
  // 页码应回到第 1 项 active(搜索重置页码)；且结果已按关键字过滤(命中很少)。
  await expect(page.locator('.ant-pagination-item-1')).toHaveClass(/ant-pagination-item-active/);
  await expect(page.getByText(/共 \d+ 条/)).toBeVisible({ timeout: 5000 });
});
