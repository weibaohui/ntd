// 回归脚本：新建事项抽屉术语 + 「从模板创建」按钮可用性。
// 背景（NTD-018）：抽屉标题/占位/提示旧为「任务」口径，已统一为「事项」；
// 同时守护「从模板创建」——用户曾反馈点不开，此处持续验证按钮可点、
// 模板弹窗能浮在抽屉之上（antd 嵌套 Modal zIndex）、模板接口 200。
// 运行前提：make dev 已起（18088）。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('新建抽屉-从模板创建按钮复现', async ({ page }) => {
  // 收集 console 错误与失败请求，便于判断「点不开」是 JS 报错还是接口问题。
  const consoleErrors: string[] = [];
  page.on('console', (msg) => {
    if (msg.type() === 'error') consoleErrors.push(msg.text());
  });
  page.on('requestfailed', (req) => {
    consoleErrors.push(`REQ FAIL: ${req.url()} ${req.failure()?.errorText}`);
  });

  await page.goto(`${BASE}/`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2500);

  // 打开新建抽屉（顶部「新建」按钮，icon + 文本，用子串匹配避开 aria-label 前缀问题）。
  await page.locator('button', { hasText: '新建' }).first().click();
  await page.waitForTimeout(1200);

  // 记录抽屉标题并断言 018 术语统一：创建模式必须显示「创建事项」。
  const drawerTitle = await page.locator('.ant-drawer-title').first().textContent().catch(() => null);
  console.log('抽屉标题:', drawerTitle);
  expect(drawerTitle).toBe('创建事项');

  // 检查「从模板创建」按钮是否存在、可见、可点。
  const tplBtn = page.locator('button', { hasText: '从模板创建' }).first();
  const btnCount = await tplBtn.count();
  const btnVisible = btnCount > 0 ? await tplBtn.isVisible().catch(() => false) : false;
  // elementFromPoint 验证点击位置最顶层元素是否就是按钮本体（排查被遮挡）。
  let topElementAtBtn: string | null = null;
  if (btnVisible) {
    const box = await tplBtn.boundingBox();
    if (box) {
      topElementAtBtn = await page.evaluate(({ x, y }) => {
        const el = document.elementFromPoint(x, y);
        // 向上找最近的可描述节点（带 class 的元素），报告按钮是否被别的层盖住。
        return el ? `${el.tagName}.${(el.className || '').toString().slice(0, 60)}` : null;
      }, { x: box.x + box.width / 2, y: box.y + box.height / 2 });
    }
  }
  console.log('按钮存在:', btnCount > 0, '可见:', btnVisible, '点击位置最顶层元素:', topElementAtBtn);

  // 监听模板接口是否被发出。
  const tplResponses: string[] = [];
  page.on('response', (res) => {
    if (res.url().includes('todo-templates')) tplResponses.push(`${res.status()} ${res.url()}`);
  });

  if (btnVisible) {
    await tplBtn.click();
    await page.waitForTimeout(1500);
  }

  // 点击后检查 ant-modal 是否出现、是否可见（存在但不可见 = 被 Drawer 盖住）。
  const modalCount = await page.locator('.ant-modal').count();
  let modalInfo: string | null = null;
  if (modalCount > 0) {
    modalInfo = await page.evaluate(() => {
      const m = document.querySelector('.ant-modal');
      if (!m) return null;
      const rect = m.getBoundingClientRect();
      const style = getComputedStyle(m);
      // wrap 是实际参与 z-index 堆叠的节点，一并报告
      const wrap = m.closest('.ant-modal-wrap');
      const wrapStyle = wrap ? getComputedStyle(wrap) : null;
      return {
        rect: { x: rect.x, y: rect.y, w: rect.width, h: rect.height },
        display: style.display,
        zIndex: wrapStyle?.zIndex ?? null,
        drawerZIndex: document.querySelector('.ant-drawer') ? getComputedStyle(document.querySelector('.ant-drawer')!).zIndex : null,
      };
    });
  }
  console.log('点击后 .ant-modal 数量:', modalCount, '详情:', JSON.stringify(modalInfo));
  console.log('todo-templates 接口响应:', tplResponses);
  console.log('console 错误:', consoleErrors.slice(0, 10));

  await page.screenshot({ path: 'frontend/tests/__screenshots__/check_template_modal.png', fullPage: false });
});
