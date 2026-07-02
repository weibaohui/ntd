// 测试执行器管理功能
const { chromium } = require('playwright');

(async () => {
  console.log('开始测试执行器管理功能...\n');

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();

  const results = {
    passed: [],
    failed: []
  };

  try {
    // 1. 导航到主页面
    console.log('1. 导航到主页面...');
    await page.goto('http://localhost:8088');
    await page.waitForLoadState('domcontentloaded');
    await page.waitForTimeout(3000);

    const title = await page.title();
    console.log(`   页面标题: ${title}`);
    results.passed.push(`页面加载成功: ${title}`);

    // 2. 点击设置图标按钮 (aria-label="配置管理")
    console.log('2. 点击配置管理按钮...');
    const settingsBtn = page.locator('[aria-label="配置管理"]');
    if (await settingsBtn.isVisible({ timeout: 5000 })) {
      await settingsBtn.click();
      await page.waitForTimeout(2000);
      results.passed.push('配置管理按钮点击成功');
      console.log('   点击了配置管理图标');
    } else {
      // 备用方法
      const settingsBtn2 = page.locator('[class*="setting"]').first();
      if (await settingsBtn2.isVisible({ timeout: 3000 })) {
        await settingsBtn2.click();
        await page.waitForTimeout(2000);
        results.passed.push('设置按钮点击成功');
      }
    }

    // 等待页面切换
    await page.waitForTimeout(1000);

    // 3. 检查是否切换到了设置页面
    console.log('3. 检查设置页面...');
    const settingsPage = page.locator('.ant-tabs');
    if (await settingsPage.isVisible({ timeout: 5000 })) {
      results.passed.push('设置页面已显示');

      // 4. 查找所有标签页
      const allTabs = await page.locator('.ant-tabs-tab').all();
      console.log(`   找到 ${allTabs.length} 个标签页`);
      for (let i = 0; i < allTabs.length; i++) {
        const tabText = await allTabs[i].textContent();
        console.log(`   标签页 ${i + 1}: ${tabText}`);
      }

      // 5. 点击"执行器管理"标签
      console.log('4. 点击执行器管理标签...');
      let tabClicked = false;

      // 通过JS点击包含"执行器"的标签
      const clicked = await page.evaluate(() => {
        const tabs = document.querySelectorAll('.ant-tabs-tab');
        for (const tab of tabs) {
          if (tab.textContent.includes('执行器')) {
            tab.click();
            return true;
          }
        }
        return false;
      });

      if (clicked) {
        tabClicked = true;
        results.passed.push('执行器管理标签点击成功');
        console.log('   执行器管理标签已点击');
        await page.waitForTimeout(1500);
      } else {
        results.failed.push('未找到执行器管理标签');
      }

      // 6. 查找 Switch 组件
      console.log('5. 查找 Switch 开关组件...');
      const allSwitches = await page.locator('.ant-switch').all();
      console.log(`   找到 ${allSwitches.length} 个 Switch`);
      if (allSwitches.length > 0) {
        results.passed.push(`找到 ${allSwitches.length} 个开关组件`);

        // 检查第一个开关的状态
        const firstSwitch = allSwitches[0];
        const isChecked = await firstSwitch.evaluate(el => el.classList.contains('ant-switch-checked'));
        console.log(`   第一个开关状态: ${isChecked ? '启用' : '禁用'}`);
        results.passed.push(`开关状态: ${isChecked ? '启用' : '禁用'}`);

        // 测试开关切换
        console.log('   测试开关切换...');
        await firstSwitch.click();
        await page.waitForTimeout(1500);

        // 检查是否有消息提示
        const msg = page.locator('.ant-message').first();
        if (await msg.isVisible({ timeout: 2000 })) {
          const msgText = await msg.textContent();
          console.log(`   消息提示: ${msgText}`);
          results.passed.push('开关切换后显示消息提示');
        }

        // 重新点击恢复原状态
        await firstSwitch.click();
        await page.waitForTimeout(1000);
      } else {
        results.failed.push('未找到 Switch 组件');
      }

      // 7. 查找检测按钮
      console.log('6. 查找检测按钮...');
      const detectBtns = await page.locator('button:has-text("检测")').all();
      console.log(`   找到 ${detectBtns.length} 个检测按钮`);
      if (detectBtns.length > 0) {
        results.passed.push(`找到 ${detectBtns.length} 个检测按钮`);

        // 点击第一个检测按钮
        console.log('   点击第一个检测按钮...');
        await detectBtns[0].click();
        await page.waitForTimeout(2000);

        // 检查结果图标
        const checkIcon = page.locator('span:has-text("✓")').first();
        const failIcon = page.locator('span:has-text("✗")').first();

        if (await checkIcon.isVisible({ timeout: 2000 }).catch(() => false)) {
          results.passed.push('检测成功显示 ✓ 图标');
        } else if (await failIcon.isVisible({ timeout: 1000 }).catch(() => false)) {
          results.passed.push('检测失败显示 ✗ 图标');
        } else {
          results.failed.push('未检测到 ✓/✗ 图标反馈');
        }
      } else {
        results.failed.push('未找到检测按钮');
      }

      // 8. 查找测试按钮
      console.log('7. 查找测试按钮...');
      const testBtns = await page.locator('button:has-text("测试")').all();
      console.log(`   找到 ${testBtns.length} 个测试按钮`);
      if (testBtns.length > 0) {
        results.passed.push(`找到 ${testBtns.length} 个测试按钮`);

        // 点击第一个测试按钮
        console.log('   点击第一个测试按钮...');
        await testBtns[0].click();
        await page.waitForTimeout(3000);

        // 检查模态框
        const modal = page.locator('.ant-modal');
        if (await modal.isVisible({ timeout: 3000 })) {
          results.passed.push('测试结果模态框已弹出');

          // 获取模态框内容
          const modalTitle = await page.locator('.ant-modal-title').textContent().catch(() => '');
          console.log(`   模态框标题: ${modalTitle}`);

          // 关闭模态框
          const closeBtn = page.locator('button:has-text("关闭")').first();
          if (await closeBtn.isVisible({ timeout: 2000 })) {
            await closeBtn.click();
            await page.waitForTimeout(500);
          }
        } else {
          results.failed.push('测试按钮点击后模态框未弹出');
        }
      } else {
        results.failed.push('未找到测试按钮');
      }

      // 9. 查找输入框
      console.log('8. 查找输入框...');
      const inputs = await page.locator('.ant-input').all();
      console.log(`   找到 ${inputs.length} 个输入框`);
      if (inputs.length > 0) {
        results.passed.push(`找到 ${inputs.length} 个输入框`);
      }

    } else {
      results.failed.push('设置页面未显示');
    }

    // 10. 截图保存
    console.log('9. 保存截图...');
    await page.screenshot({ path: '/tmp/executor_config_test.png', fullPage: true });
    results.passed.push('截图已保存');

  } catch (error) {
    console.error('\n测试过程中出错:', error.message);
    results.failed.push(`测试异常: ${error.message}`);
    await page.screenshot({ path: '/tmp/executor_config_error.png' }).catch(() => {});
  }

  console.log('\n========================================');
  console.log('测试结果汇总');
  console.log('========================================');

  console.log(`\n✅ 通过 (${results.passed.length}):`);
  results.passed.forEach(item => console.log(`  • ${item}`));

  if (results.failed.length > 0) {
    console.log(`\n❌ 失败 (${results.failed.length}):`);
    results.failed.forEach(item => console.log(`  • ${item}`));
  }

  console.log('\n========================================');
  console.log(`总计: ${results.passed.length} 通过, ${results.failed.length} 失败`);
  console.log('========================================\n');

  await browser.close();
  process.exit(results.failed.length > 0 ? 1 : 0);
})();