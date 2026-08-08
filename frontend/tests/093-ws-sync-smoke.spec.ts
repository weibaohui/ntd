// 093 WS 重连日志尾取（后端）冒烟：验证 Sync 事件链路在新 DAO 下工作正常。
// 覆盖点：WS 建连收到 Sync（新 DAO 路径：尾部窗口查询 + COUNT 聚合）、页面无报错渲染。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('093-ws: WS 建连收到 Sync 事件且页面正常渲染', async ({ page }) => {
  const pageErrors: string[] = [];
  page.on('pageerror', (e) => pageErrors.push(e.message));

  // 在页面加载前注入 WS 抓包：包装 WebSocket 构造函数记录收到的 Sync 帧
  await page.addInitScript(() => {
    (window as any).__syncFrames = 0;
    const OrigWS = window.WebSocket;
    (window as any).WebSocket = class extends OrigWS {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(url, protocols);
        this.addEventListener('message', (ev) => {
          // Sync 帧是 JSON 且 type=Sync；握手 "Connected" 文本帧跳过
          if (typeof ev.data === 'string' && ev.data.includes('"type":"Sync"')) {
            (window as any).__syncFrames += 1;
          }
        });
      }
    };
  });

  await page.goto(BASE);
  await expect(page.locator('#root')).not.toBeEmpty({ timeout: 15000 });

  // 等待 WS 建连并收到 Sync 快照（走新 DAO：空任务集也应正常发 Sync 帧）
  await page.waitForFunction(() => (window as any).__syncFrames > 0, null, { timeout: 15000 });
  expect(pageErrors).toEqual([]);
});
