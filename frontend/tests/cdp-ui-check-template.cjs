#!/usr/bin/env node
/**
 * 107：CDP 真实点击验收模板（可复用）。
 *
 * 用途：通过 CDP 连接真实 Chrome（9222 端口），按步骤序列做真实点击/输入/断言，
 * 供 AI 对自动化 spec 覆盖不到的「视觉/交互观感」功能做验收回归。
 * 每次使用只改 --url 与 --steps，无需重写脚本。
 *
 * 用法：
 *   node tests/cdp-ui-check-template.cjs \
 *     --url "http://localhost:18088/#/processes" \
 *     --steps '[{"action":"expectText","text":"工艺"},{"action":"click","selector":"button:has-text(\"详情\")"},{"action":"expectText","text":"流程图"},{"action":"screenshot","name":"detail.png"}]'
 *
 * 支持动作：
 *   - goto <url>                 （可选，覆盖 --url）
 *   - wait <ms>
 *   - expectText <text>          （页面包含文本）
 *   - click <selector>           （Playwright 选择器）
 *   - fill <selector>|<text>     （填输入框）
 *   - expectVisible <selector>
 *   - screenshot <name>          （存 /tmp/cdp-shots/<name>）
 *
 * 环境前提（见 MEMORY.md）：
 *   Chrome 以 --remote-debugging-port=9222 --user-data-dir="$HOME/.chrome-cdp-profile" 启动；
 *   curl http://127.0.0.1:9222/json/version 验证。
 * 已知坑防护：自动移除 .ant-tooltip-container（悬停残留遮挡点击）。
 *
 * 退出码：0 全部通过；1 任一断言失败。
 */
const { chromium } = require('@playwright/test');

function parseArgs(argv) {
  const args = { url: null, steps: null };
  for (let i = 2; i < argv.length; i += 2) {
    if (argv[i] === '--url') args.url = argv[i + 1];
    if (argv[i] === '--steps') args.steps = JSON.parse(argv[i + 1]);
  }
  return args;
}

async function main() {
  const { url, steps } = parseArgs(process.argv);
  if (!steps || !Array.isArray(steps)) {
    console.error('用法: node tests/cdp-ui-check-template.cjs --url <url> --steps \'[...]\'');
    process.exit(2);
  }

  // 连接 CDP 9222 上的 Chrome（真实浏览器，非 headless）。
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
  const ctx = browser.contexts()[0];
  // --url 传入时新建标签页并导航；否则复用当前 ntd 页面。
  let page = url ? await ctx.newPage() : ctx.pages().find((p) => p.url().includes('18088'));
  if (!page) { console.error('无可用页面，请先打开 ntd 页面或传 --url'); process.exit(2); }
  if (url) {
    await page.goto(url, { waitUntil: 'domcontentloaded' });
    console.log(`[init] goto ${url}`);
  }

  let failed = 0;
  for (let i = 0; i < steps.length; i++) {
    const s = steps[i];
    const stepNo = `[${i + 1}/${steps.length}]`;
    try {
      switch (s.action) {
        case 'goto':
          await page.goto(s.url ?? s.value, { waitUntil: 'domcontentloaded' });
          console.log(`${stepNo} goto ${s.url ?? s.value} ✅`);
          break;
        case 'wait':
          await page.waitForTimeout(s.ms ?? s.value ?? 1000);
          console.log(`${stepNo} wait ${s.ms ?? s.value ?? 1000}ms ✅`);
          break;
        case 'expectText': {
          // 已知坑防护：移除 Tooltip 残留，避免遮挡后续点击。
          await page.evaluate(() => document.querySelectorAll('.ant-tooltip-container').forEach((el) => el.remove()));
          // 轮询等待文本出现（SPA 首屏/数据加载可能慢于固定等待）。
          await page.waitForFunction(
            (t) => document.body.innerText.includes(t),
            s.text,
            { timeout: 10000 },
          );
          console.log(`${stepNo} expectText「${s.text}」✅`);
          break;
        }
        case 'click': {
          await page.evaluate(() => document.querySelectorAll('.ant-tooltip-container').forEach((el) => el.remove()));
          const el = page.locator(s.selector).first();
          await el.waitFor({ state: 'visible', timeout: 8000 });
          await el.click({ force: true });
          console.log(`${stepNo} click ${s.selector} ✅`);
          break;
        }
        case 'fill': {
          const [sel, text] = s.selector.split('|');
          await page.locator(sel).first().fill(text);
          console.log(`${stepNo} fill ${sel} ✅`);
          break;
        }
        case 'expectVisible': {
          const el = page.locator(s.selector).first();
          await el.waitFor({ state: 'visible', timeout: 8000 });
          console.log(`${stepNo} expectVisible ${s.selector} ✅`);
          break;
        }
        case 'screenshot': {
          const fs = require('fs');
          const dir = '/tmp/cdp-shots';
          fs.mkdirSync(dir, { recursive: true });
          await page.screenshot({ path: `${dir}/${s.name}` });
          console.log(`${stepNo} screenshot → ${dir}/${s.name} ✅`);
          break;
        }
        default:
          throw new Error(`未知动作 ${s.action}`);
      }
    } catch (e) {
      failed++;
      console.error(`${stepNo} ${s.action} ❌ ${e.message}`);
      try { await page.screenshot({ path: `/tmp/cdp-shots/fail-${i + 1}.png` }); } catch {}
    }
    // 动作间留出渲染时间。
    await page.waitForTimeout(500);
  }

  await browser.close();
  console.log(failed === 0 ? '\n✅ 全部检查通过' : `\n❌ ${failed} 项检查失败`);
  process.exit(failed === 0 ? 0 : 1);
}

main().catch((e) => { console.error('执行异常:', e.message); process.exit(1); });
