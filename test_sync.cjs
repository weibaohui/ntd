const { chromium } = require('@playwright/test');

(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext();
  const page = await context.newPage();

  const errors = [];
  page.on('console', msg => {
    if (msg.type() === 'error') errors.push(msg.text());
  });

  try {
    console.log('1. 访问应用...');
    await page.goto('http://localhost:18088');
    await page.waitForTimeout(3000);

    console.log('2. 点击设置图标...');
    await page.locator('.anticon-setting').click();
    await page.waitForTimeout(1500);

    console.log('3. 点击云端同步 tab...');
    await page.locator('.ant-tabs-tab:has-text("云端同步")').click();
    await page.waitForTimeout(1500);

    console.log('4. 填写服务器地址...');
    await page.locator('.cloud-sync-panel input[placeholder="http://localhost:8089"]').fill('http://localhost:8089');

    console.log('5. 填写 Token...');
    await page.locator('.cloud-sync-panel input[placeholder="ntd_xxx 格式的同步 Token"]').fill('ntd_291df6c5-2f39-431c-b269-867406ed9e39');

    console.log('6. 点击保存配置...');
    await page.locator('.cloud-sync-panel button:has-text("保存配置")').click();
    await page.waitForTimeout(2000);

    console.log('7. 检查状态...');
    const success = await page.locator('.cloud-sync-panel .ant-alert-success').isVisible();
    console.log(success ? '✓ 已连接' : '⚠ 未连接');

    console.log('8. 点击推送按钮...');
    await page.locator('.cloud-sync-panel button:has-text("推送")').click();
    await page.waitForTimeout(1000);

    console.log('9. 检查弹窗...');
    const modal = await page.locator('.ant-modal').isVisible();
    console.log(modal ? '✓ 弹窗显示' : '⚠ 弹窗未显示');

    if (modal) {
      console.log('10. 检查弹窗标题...');
      const title = await page.locator('.ant-modal-title').textContent();
      console.log('弹窗标题:', title);

      console.log('11. 检查策略选项...');
      const options = await page.locator('.ant-modal .ant-select-item').count();
      console.log('策略选项数量:', options);

      console.log('12. 点击执行同步...');
      await page.locator('.ant-modal button:has-text("执行同步")').click();
      await page.waitForTimeout(3000);

      console.log('13. 检查消息提示...');
      const msg = await page.locator('.ant-message').isVisible();
      console.log(msg ? '✓ 显示消息' : '⚠ 未显示消息');
    }

    if (errors.length > 0) {
      console.log('\n⚠ Console errors:', errors.slice(0, 3));
    } else {
      console.log('✓ 无 console 错误');
    }

    console.log('\n=== 测试完成 ===');
  } catch (e) {
    console.log('测试失败:', e.message);
  }

  await browser.close();
})();
