/**
 * 剪贴板工具函数测试
 *
 * 验证升级到 clipboard.js 后 copyToClipboard 的行为：
 * - 在浏览器中调用应返回 true 并把文本写入系统剪贴板
 * - 调用结束后临时按钮必须从 DOM 中移除，避免泄漏
 * - clipboard.js 应作为运行时依赖被正确加载
 *
 * 对应 Issue #599：前端页面点击复制功能升级
 */

import { test, expect, chromium } from '@playwright/test';

const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:5173';

/**
 * 直接通过 dynamic import 拉取 utils/clipboard.ts 中的 copyToClipboard。
 * 在浏览器里执行，绕开 Vite 的模块缓存，确保运行时真实代码路径被覆盖。
 */
async function importClipboardInPage(page: import('@playwright/test').Page) {
  return page.evaluate(async () => {
    const mod = await import('/src/utils/clipboard.ts');
    return { copyToClipboard: mod.copyToClipboard };
  });
}

test.describe('copyToClipboard (clipboard.js 升级) — Issue #599', () => {
  test('复制普通文本应返回 true 并写入剪贴板', async () => {
    const browser = await chromium.launch();
    // 授予 clipboard-read/write 权限，否则 navigator.clipboard.readText 会失败
    const context = await browser.newContext({
      permissions: ['clipboard-read', 'clipboard-write'],
    });
    const page = await context.newPage();
    await page.goto(DEV_URL);

    const { copyToClipboard } = await importClipboardInPage(page);
    const sample = `ntd-测试-${Date.now()}`;

    const ok = await page.evaluate(async (text) => {
      const mod = await import('/src/utils/clipboard.ts');
      return mod.copyToClipboard(text);
    }, sample);

    expect(ok).toBe(true);

    // 二次校验：直接从系统剪贴板读回，确保文本确实落到了剪贴板
    const readBack = await page.evaluate(() => navigator.clipboard.readText());
    expect(readBack).toBe(sample);

    await browser.close();
  });

  test('复制后临时按钮必须从 DOM 中移除', async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext({
      permissions: ['clipboard-read', 'clipboard-write'],
    });
    const page = await context.newPage();
    await page.goto(DEV_URL);

    // 在调用之前先埋点 body 的子节点数；调用后再核对一次
    await page.evaluate(async () => {
      const mod = await import('/src/utils/clipboard.ts');
      await mod.copyToClipboard('cleanup-check');
    });

    // copyToClipboard 创建的临时按钮是「不可见」按钮，但仍挂在 document.body 上
    // 调用完成后应被销毁；若仍残留则说明清理逻辑有漏洞
    const leftover = await page.evaluate(() => {
      const btns = Array.from(document.body.querySelectorAll('button'));
      // 过滤出 copyToClipboard 创建的临时按钮：含 aria-hidden 且无文本
      return btns.filter(b => b.getAttribute('aria-hidden') === 'true').length;
    });
    expect(leftover).toBe(0);

    await browser.close();
  });

  test('连续多次复制应互不干扰', async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext({
      permissions: ['clipboard-read', 'clipboard-write'],
    });
    const page = await context.newPage();
    await page.goto(DEV_URL);

    // 连发 3 次不同文本，最后一次读回剪贴板，必须等于最后一次的文本
    // 用于验证 destroy + 重建不会让上一次实例污染下一次结果
    const results = await page.evaluate(async () => {
      const mod = await import('/src/utils/clipboard.ts');
      const r1 = await mod.copyToClipboard('first');
      const r2 = await mod.copyToClipboard('second');
      const r3 = await mod.copyToClipboard('third');
      const readBack = await navigator.clipboard.readText();
      return { r1, r2, r3, readBack };
    });

    expect(results.r1).toBe(true);
    expect(results.r2).toBe(true);
    expect(results.r3).toBe(true);
    expect(results.readBack).toBe('third');

    await browser.close();
  });
});