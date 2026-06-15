import { defineConfig } from '@playwright/test';

export default defineConfig({
  // 统一收口到 frontend/tests/，与 CLAUDE.md「前端测试脚本位置」保持一致
  testDir: './tests',
  // 同时跑正式 spec 与 check_*.cjs / check_*.js 调试脚本
  testMatch: ['**/*.spec.ts', '**/check_*.cjs', '**/check_*.js'],
  timeout: 30000,
  use: {
    headless: true,
    baseURL: 'http://localhost:5173',
  },
});