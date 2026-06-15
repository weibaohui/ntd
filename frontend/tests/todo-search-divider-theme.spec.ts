/**
 * todo 搜索栏分隔线主题色测试
 *
 * 验证修复后的搜索栏下方横线在亮/暗主题下颜色都能跟随主题：
 * - 亮色主题下颜色应明显浅于暗色主题（不再是固定白色 #f0f0f0）
 * - 暗色主题下颜色应足够深，不再突兀
 * - 同一会话切换主题时颜色发生变化
 *
 * 对应 Issue #602：todo 搜索框主题色修正
 */

import { test, expect, chromium, type Page } from '@playwright/test';

const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:5173';

// 解析 rgb()/rgba() 字符串为对象，便于做亮度判定
function parseRgb(rgbStr: string): { r: number; g: number; b: number } | null {
  const m = rgbStr.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
  if (!m) return null;
  return { r: parseInt(m[1], 10), g: parseInt(m[2], 10), b: parseInt(m[3], 10) };
}

// 在页面上定位 todo 搜索框所在的容器 div（带 border-bottom: 1px solid）并返回颜色
async function getSearchDividerColor(page: Page): Promise<string | null> {
  return await page.evaluate(() => {
    // 通过 placeholder 文本定位搜索框；向上回溯到第一个有可见 border-bottom 的祖先
    const inputs = Array.from(document.querySelectorAll('input'));
    const searchInput = inputs.find(
      (i) => i.placeholder && i.placeholder.includes('搜索标题'),
    );
    if (!searchInput) return null;
    let el: HTMLElement | null = searchInput.parentElement;
    while (el && el !== document.body) {
      const style = getComputedStyle(el);
      if (style.borderBottomStyle === 'solid' && style.borderBottomWidth !== '0px') {
        return style.borderBottomColor;
      }
      el = el.parentElement;
    }
    return null;
  });
}

test.describe('todo 搜索栏分隔线主题色 — Issue #602', () => {
  test('亮色主题下分隔线颜色为浅色（非纯白）', async () => {
    const browser = await chromium.launch();
    // 强制 light colorScheme，避免系统暗色偏好影响 useTheme 初始判断
    const context = await browser.newContext({ colorScheme: 'light' });
    const page = await context.newPage();
    await page.goto(DEV_URL);
    // 显式写入 localStorage 锁定主题，刷新后由 ThemeProvider 接管
    await page.evaluate(() => localStorage.setItem('app_theme', 'light'));
    await page.reload();
    // 等待页面布局稳定，给 useLayoutEffect 写入 data-theme 的时间
    await page.waitForTimeout(1500);

    const color = await getSearchDividerColor(page);
    expect(color).not.toBeNull();
    const rgb = parseRgb(color!);
    expect(rgb).not.toBeNull();
    // 修复前的硬编码 #f0f0f0 → rgb(240,240,240)，三通道和=720，过浅
    // 修复后用 --color-border-light=#f1f5f9 → rgb(241,245,249)，同样浅
    // 但关键是不能等于纯白 (255,255,255)；且比暗色应明显更浅
    expect(rgb!.r + rgb!.g + rgb!.b).toBeLessThan(255 * 3);
    expect(rgb!.r + rgb!.g + rgb!.b).toBeGreaterThan(600); // 至少是浅灰

    await browser.close();
  });

  test('暗色主题下分隔线颜色为深色（不再突兀的浅线）', async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext({ colorScheme: 'dark' });
    const page = await context.newPage();
    await page.goto(DEV_URL);
    await page.evaluate(() => localStorage.setItem('app_theme', 'dark'));
    await page.reload();
    await page.waitForTimeout(1500);

    const color = await getSearchDividerColor(page);
    expect(color).not.toBeNull();
    const rgb = parseRgb(color!);
    expect(rgb).not.toBeNull();
    // 修复后 --color-border-light=#262637 → rgb(38,38,55)，三通道和=131
    // 修复前 #f0f0f0 仍是 720，所以三通道和应显著低于 720
    expect(rgb!.r + rgb!.g + rgb!.b).toBeLessThan(300);

    await browser.close();
  });

  test('同一会话切换主题时颜色发生变化', async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext({ colorScheme: 'light' });
    const page = await context.newPage();
    await page.goto(DEV_URL);
    await page.evaluate(() => localStorage.setItem('app_theme', 'light'));
    await page.reload();
    await page.waitForTimeout(1500);

    const lightColor = await getSearchDividerColor(page);
    expect(lightColor).not.toBeNull();

    // 切换到暗色并刷新，让 ThemeProvider 重新读 localStorage
    await page.evaluate(() => localStorage.setItem('app_theme', 'dark'));
    await page.reload();
    await page.waitForTimeout(1500);

    const darkColor = await getSearchDividerColor(page);
    expect(darkColor).not.toBeNull();

    // 核心断言：颜色必须跟随主题变化；修复前两边都是 #f0f0f0 永远相等
    expect(lightColor).not.toBe(darkColor);

    // 暗色必须明显比亮色暗
    const lr = parseRgb(lightColor!)!;
    const dr = parseRgb(darkColor!)!;
    expect(dr.r + dr.g + dr.b).toBeLessThan(lr.r + lr.g + lr.b);

    await browser.close();
  });
});