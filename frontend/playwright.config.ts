import { defineConfig } from '@playwright/test';

export default defineConfig({
  // 统一收口到 frontend/tests/，与 CLAUDE.md「前端测试脚本位置」保持一致
  testDir: './tests',
  // 只匹配 Playwright 标准 spec（含 check_xxx.spec.ts 形式的调试用例）。
  // 不纳入 check_*.cjs / check_*.js：它们是依赖外部 CLI（如 agent-browser）的独立
  // Node 脚本，需手动 `node xxx.cjs` 运行；若被 testMatch 匹配，Playwright 加载时会在
  // 文件顶层执行其 main()，因缺失 CLI 而抛错，导致整套件在跑任何用例前就崩溃。
  testMatch: ['**/*.spec.ts'],
  timeout: 30000,
  use: {
    headless: true,
    // dev 服务用 embedded 模式（前端 dist 经 rust-embed 打进后端），监听 18088；
    // spec 中相对路径 page.goto('/#/...') 据此拼接，必须对齐 dev 端口。
    baseURL: 'http://localhost:18088',
  },
});