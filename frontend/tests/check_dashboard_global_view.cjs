#!/usr/bin/env node
// Dashboard 全局视图菜单位置回归验证脚本。
//
// 覆盖：
// - 左侧导航出现独立的「全局视图」分组，「仪表盘」位于其中
// - 点击仪表盘进入 /#/dashboard
// - 切换工作区后 Dashboard 保持打开
//
// 运行前需先启动开发服务：make dev
// 运行：node frontend/tests/check_dashboard_global_view.cjs

const { execSync } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

const SESSION = process.env.PILOT_SESSION_ID || 'default';
const APP_URL = process.env.E2E_BASE_URL || 'http://localhost:18088';
const SCREENSHOT_DIR = path.join(__dirname, '__screenshots__');

function run(cmd) {
  console.log(`> ${cmd}`);
  return execSync(cmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'inherit'] });
}

function hasText(output, text) {
  if (!output.includes(text)) {
    throw new Error(`Expected snapshot to include "${text}", got:\n${output}`);
  }
}

function main() {
  fs.mkdirSync(SCREENSHOT_DIR, { recursive: true });

  const sessionArgs = `--session ${SESSION}`;
  const screenshot1 = path.join(SCREENSHOT_DIR, 'dashboard-global-menu.png');
  const screenshot2 = path.join(SCREENSHOT_DIR, 'dashboard-after-switch-workspace.png');

  try {
    // 1. 打开应用并截图
    run(`agent-browser ${sessionArgs} open ${APP_URL}`);
    run(`agent-browser ${sessionArgs} wait 2000`);
    run(`agent-browser ${sessionArgs} screenshot ${screenshot1}`);

    // 2. 展开左侧导航，检查「全局视图」和「仪表盘」
    // 先尝试展开导航（如果默认收起）
    try {
      run(`agent-browser ${sessionArgs} click "[data-testid='left-rail-toggle']"`);
      run(`agent-browser ${sessionArgs} wait 500`);
    } catch {
      // 可能已经是展开状态，忽略
    }

    const snapshot1 = run(`agent-browser ${sessionArgs} snapshot -i`);
    hasText(snapshot1, '全局视图');
    hasText(snapshot1, '仪表盘');

    // 3. 点击仪表盘
    run(`agent-browser ${sessionArgs} click "[data-testid='left-rail-dashboard']"`);
    run(`agent-browser ${sessionArgs} wait 2000`);

    const url1 = run(`agent-browser ${sessionArgs} get url`).trim();
    if (!url1.includes('/dashboard')) {
      throw new Error(`Expected URL to include /dashboard, got: ${url1}`);
    }

    // 4. 切换工作区（选择第一个可用工作区）
    run(`agent-browser ${sessionArgs} click "[data-testid='left-rail-workspace-switcher']"`);
    run(`agent-browser ${sessionArgs} wait 500`);
    const menuItems = run(`agent-browser ${sessionArgs} eval "Array.from(document.querySelectorAll('.ant-dropdown-menu-item')).map(el => el.textContent.trim())"`);
    if (menuItems.includes('临时工作空间')) {
      run(`agent-browser ${sessionArgs} eval "document.querySelector('.ant-dropdown-menu-item').click()"`);
      run(`agent-browser ${sessionArgs} wait 1000`);
    }

    run(`agent-browser ${sessionArgs} screenshot ${screenshot2}`);

    const url2 = run(`agent-browser ${sessionArgs} get url`).trim();
    if (!url2.includes('/dashboard')) {
      throw new Error(`Expected URL to still include /dashboard after workspace switch, got: ${url2}`);
    }

    console.log('\n✅ Dashboard 全局视图验证通过');
    console.log(`   截图 1: ${screenshot1}`);
    console.log(`   截图 2: ${screenshot2}`);
  } finally {
    run(`agent-browser ${sessionArgs} close`);
  }
}

main();
