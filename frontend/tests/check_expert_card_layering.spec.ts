import { test, expect, type Page } from '@playwright/test';

// 验证专家 / 专家团队卡片在浅色与深色模式下都有清晰描边与常态阴影，
// 解决「卡片边框与背景同色、看不出来、没有层次」的问题。
//
// 修复前根因：--color-border-secondary 从未定义，导致卡片
//   border: '1px solid var(--color-border-secondary)'
// 整条声明在计算时失效（border-style 退回初值 none、无描边），
// 且卡片背景与父容器同为 --color-bg-elevated，又没有常态阴影，
// 于是卡片与背景融为一体，只有 hover 才出阴影。
//
// 修复后：定义了 --color-border-secondary，并给卡片常态加 var(--shadow-sm)。
// 因此可观测的不变量是：borderStyle === 'solid'（而非 none）、boxShadow !== 'none'。
const BASE = 'http://localhost:18088';
const THEMES = ['light', 'dark'] as const;

// 取当前激活 tab 下的第一张卡片。专家/团队卡片底部都有「N 项技能」，
// 用该文案配合 role=button 定位，可同时命中两种卡片。
async function readFirstCardStyle(page: Page) {
  const card = page
    .locator('.ant-tabs-tabpane-active')
    .getByRole('button', { name: /项技能/ })
    .first();
  await card.waitFor({ state: 'visible', timeout: 10000 });
  // 读计算样式：var(--shadow-sm) 等变量会被 getComputedStyle 解析成真实值，
  // 据此判断描边与阴影是否真的渲染出来。
  return card.evaluate((el) => {
    const cs = getComputedStyle(el);
    return { borderStyle: cs.borderStyle, borderColor: cs.borderColor, boxShadow: cs.boxShadow };
  });
}

for (const theme of THEMES) {
  test(`专家/团队卡片在${theme === 'dark' ? '深色' : '浅色'}模式下有清晰描边与常态阴影`, async ({ page }) => {
    test.setTimeout(60000);
    // 首帧前写入主题，让 ThemeProvider 初始化即进入目标主题，避免闪烁默认主题干扰断言。
    await page.addInitScript((t) => localStorage.setItem('app_theme', t), theme);
    await page.goto(`${BASE}/#/experts`);
    // bundled 内置专家由后端提供，给一点加载时间。
    await page.waitForTimeout(1500);

    // 默认即「专家」tab：校验专家卡片。
    const expert = await readFirstCardStyle(page);
    console.log(`[${theme}/专家]`, expert);
    expect(expert.borderStyle, '专家卡片常态描边应为 solid（修复前为 none）').toBe('solid');
    expect(expert.borderColor, '专家卡片描边不应透明').not.toBe('rgba(0, 0, 0, 0)');
    expect(expert.boxShadow, '专家卡片常态应有阴影层次（修复前为 none）').not.toBe('none');

    // 切到「专家团队」tab：校验团队卡片。
    await page.getByRole('tab', { name: '专家团队' }).click();
    await page.waitForTimeout(600);
    const team = await readFirstCardStyle(page);
    console.log(`[${theme}/团队]`, team);
    expect(team.borderStyle, '团队卡片常态描边应为 solid').toBe('solid');
    expect(team.borderColor, '团队卡片描边不应透明').not.toBe('rgba(0, 0, 0, 0)');
    expect(team.boxShadow, '团队卡片常态应有阴影层次').not.toBe('none');
  });
}
