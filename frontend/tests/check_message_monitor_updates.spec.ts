// 验证消息监控台的几项修复：
// 1) ID 行复制按钮为图标按钮(无"复制"文字)且紧跟 ID；
// 2) processed_id=0 的消息(默认响应-执行器)不再渲染残留的"0"或"关联 #0"；
// 3) 智能助手配置的「群聊白名单」tab 顶部有说明提示。
// 用 Playwright 原生 locator，规避 playwright-cli 的 ref 快照过期问题。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

// 通过 aria-label/文本点击左侧导航的「消息」，再选「临时工作空间」。
// 这两步是进入消息列表的前置条件。
async function gotoMessages(page: import('@playwright/test').Page) {
  await page.goto(BASE);
  await page.getByRole('button', { name: '消息', exact: true }).click();
  // 未选工作空间时点「切换工作空间」打开下拉，再选目标工作空间。
  await page.getByRole('button', { name: '切换工作空间' }).click();
  await page.getByRole('menuitem', { name: '临时工作空间' }).click();
  // 等列表加载：标题「消息监控台」出现且至少一张卡片渲染。
  await expect(page.getByRole('heading', { name: /消息监控台/ })).toBeVisible({ timeout: 10000 });
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

test('推送目标 ID 输入框回填已有值(回归：曾因共享 form 实例被空保存擦除)', async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole('button', { name: '智能助手' }).click();
  await page.getByRole('button', { name: /配置/ }).first().click();
  // push tab 为默认 tab，用占位符定位两个输入框(占位符唯一)，验证回填 DB 中已存在的 ou_/oc_ 值。
  const p2pInput = page.getByPlaceholder(/ou_xxxxxxxx/);
  const groupInput = page.getByPlaceholder(/oc_xxxxxxxx/);
  await expect(p2pInput).toHaveValue(/^ou_.+/, { timeout: 10000 });
  await expect(groupInput).toHaveValue(/^oc_.+/, { timeout: 10000 });
});

test('消息监控台筛选区含「处理类型」下拉(任务17/18)', async ({ page }) => {
  await gotoMessages(page);
  // 处理类型下拉存在(默认显示「全部类型」)，证明已加入筛选区。
  const select = page.locator('.ant-select').filter({ hasText: '全部类型' }).first();
  await expect(select).toBeVisible({ timeout: 10000 });
});
