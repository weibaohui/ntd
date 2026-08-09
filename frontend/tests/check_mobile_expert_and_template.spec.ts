import { test, expect, type Page } from '@playwright/test';

// 验证两个手机端排版修复：
// 1. 专家详情卡片：profession 不再被挤成竖排，宽度充足横向展示。
// 2. 事项模板表格：窄屏启用横向滚动，列不再被压缩错位(此前漏配 scroll)。
const BASE = 'http://localhost:18088';

// 手机视口(iPhone X 尺寸)，触发 useIsMobile(阈值 768)的手机分支。
test.use({ viewport: { width: 375, height: 812 } });

// 点开一张「含 profession」的专家卡片并返回该元素。
// 个别专家可能缺 profession 字段，故最多遍历前 4 张，确保取到可验证样本。
async function openExpertWithProfession(page: Page) {
  const cards = page.getByRole('button', { name: /项技能/ });
  await cards.first().waitFor({ state: 'visible', timeout: 10000 });
  const total = await cards.count();
  for (let i = 0; i < Math.min(total, 4); i++) {
    await cards.nth(i).click();
    await expect(page.locator('.ant-modal').last()).toBeVisible({ timeout: 8000 });
    const profession = page.getByTestId('expert-profession');
    // 找到含 profession 的卡片即返回；否则关闭 modal 继续试下一张。
    if (await profession.count() > 0) {
      return profession;
    }
    await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
  }
  throw new Error('前 4 张专家均无 profession，无法验证');
}

test('专家详情卡片：手机端 profession 横向不竖排', async ({ page }) => {
  test.setTimeout(60000);
  await page.goto(`${BASE}/#/experts`);
  // 等专家卡片列表加载(系统内置专家由后端 bundled 提供)。
  await page.waitForTimeout(2000);

  const profession = await openExpertWithProfession(page);
  await expect(profession).toBeVisible();

  // 修复前：header 横排，文字列被挤到几像素宽，profession 逐字换行成竖排。
  // 修复后：header 纵向堆叠，文字列占满整行宽度，profession 横向展示（实测列宽≈175px）。
  //
  // 关键坑：antd Modal 进场用 transform: scale() 做整体缩放动画，toBeVisible 在动画
  // 期间就返回 true。此刻读 boundingBox 拿到的是被等比缩小的尺寸——profession 列宽会被
  // 缩到 100px 以下，直接断言 width>100 会偶发失败（实测表现为本用例时过时不过）。
  // 注意：等比缩放下「高<宽」的比例恒成立，所以 height<width 这一条无法捕获动画中的
  // 假阴性；必须对绝对宽度做轮询，等动画收完、宽度回到真实列宽再下断言。
  await expect.poll(async () => (await profession.boundingBox())?.width ?? 0, {
    timeout: 5000,
    message: 'profession 宽度应等 Modal 进场动画结束并横向展开',
  }).toBeGreaterThan(100);

  // 动画收完后再取最终尺寸，做横排/竖排语义断言。
  const box = await profession.boundingBox();
  expect(box, 'profession 元素应存在尺寸').not.toBeNull();
  // 横排一行时高度≈单行行高(~19px)，远小于列宽；竖排逐字换行时高度≈字数×行高，
  // 会反超宽度。用 height<Width 作为语义判据，比任何固定像素阈值都更稳。
  expect(box!.height, 'profession 高度应为一行，非逐字换行的多行').toBeLessThan(box!.width);
});

test('事项模板表格：手机端启用横向滚动，列不再被压缩错位', async ({ page }) => {
  test.setTimeout(60000);
  // 用 URL ?tab=templates 直接激活「模板管理」tab，绕开窄屏下设置页 7 个 tab
  // 横向溢出、点击不可靠的问题(等价于用户手动点到该 tab)。
  await page.goto(`${BASE}/#/settings?tab=templates`);
  await page.waitForTimeout(1500);
  await expect(page.locator('.ntd-templates-panel')).toBeVisible({ timeout: 10000 });
  // 子 tab 只有「专家模板/事项模板」两个，窄屏放得下；用 class+文本定位，避免 Badge 干扰 name 计算。
  await page.locator('.ntd-templates-panel .ant-tabs-tab').filter({ hasText: '事项模板' }).click();

  // 用专属 className 定位事项模板容器，避开嵌套 Tabs 中其它(隐藏)表格的干扰。
  const todoTab = page.locator('.todo-templates-tab');
  await expect(todoTab).toBeVisible({ timeout: 10000 });

  // 确保表格至少一行：无数据则新建一个模板(bundled 通常已有内置模板，此处兜底)。
  if ((await todoTab.locator('.ant-table-tbody tr.ant-table-row').count()) === 0) {
    await todoTab.getByRole('button', { name: /新建模板/ }).click();
    await page.waitForTimeout(500);
    await page.getByPlaceholder('模板标题').fill('pw-cols-test');
    // 填较长 prompt，确保 Prompt 列列宽被体现、触发横向滚动。
    await page.getByPlaceholder('模板的 AI prompt 内容')
      .fill('playwright 列对齐验证用 prompt，需足够长度体现列宽');
    await page.locator('.ant-modal-footer').getByRole('button', { name: '确定' }).click();
    await page.waitForTimeout(1200);
  }

  const table = todoTab.locator('.ant-table').first();
  await expect(table).toBeVisible({ timeout: 10000 });

  // 核心断言：修复后 <Table> 带 scroll={{ x: 'max-content' }}，内容超出容器可横向滚动。
  // 修复前无 scroll，列被压缩进容器，scrollWidth ≈ clientWidth；修复后超出、可横滑。
  // 不同 antd 版本滚动容器类名为 .ant-table-body 或 .ant-table-content，统一在表格内取。
  const scrollInfo = await table.evaluate((el) => {
    const scroller = el.querySelector('.ant-table-body') || el.querySelector('.ant-table-content') || el;
    return { cls: scroller.className, sw: scroller.scrollWidth, cw: scroller.clientWidth };
  });
  console.log('[表格滚动容器]', scrollInfo.cls, `scrollWidth=${scrollInfo.sw} clientWidth=${scrollInfo.cw}`);
  expect(scrollInfo.sw, '窄屏应启用横向滚动(scroll)，而非压缩列').toBeGreaterThan(scrollInfo.cw);
});
