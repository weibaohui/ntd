// 本机 Playwright 本地运行配置：复用仓库基线配置，仅把浏览器改为系统 Chrome。
//
// 用途：Playwright 官方 CDN 在本机不可达、无法下载 chromium 时的本地验证通道
// （`npx playwright test --config=tests/playwright.chrome.config.ts`）。
// CI/他人环境仍用根目录 playwright.config.ts（下载官方 chromium）。
//
// 注意：本文件不匹配 testMatch（非 *.spec.ts），不会被默认套件加载。

import { defineConfig } from '@playwright/test';
import base from '../playwright.config';

export default defineConfig({
  ...base,
  // 显式重写 testDir：本配置文件位于 tests/ 下，基线的相对 testDir 会解析错位
  testDir: './',
  testMatch: ['**/*.spec.ts'],
  use: {
    ...base.use,
    channel: 'chrome',
  },
});
