/**
 * issue #648 — 浏览器侧 mount 脚本
 *
 * Playwright 加载 issue-648-mount.html，Vite dev server 把这个 .ts 文件
 * 当成 module 解析，import map / bare specifier 自动注入。
 *
 * 流程：
 * 1) 从 URL hash 读 logs / executor（测试代码通过 hash 注入）
 * 2) createRoot 挂 CommandPanel 到 #test-command-panel
 * 3) 写 window.__renderDone = true，让 Playwright 收尾
 */
import React from 'react';
import { createRoot } from 'react-dom/client';
import { CommandPanel } from '../src/components/CommandPanel.tsx';

interface MountData {
  logs: Array<Record<string, unknown>>;
  executor: string;
}

interface MountWindow extends Window {
  __renderDone?: boolean;
  __renderError?: string;
}

const w = window as MountWindow;

function readDataFromHash(): MountData {
  const h = window.location.hash.replace(/^#/, '');
  if (!h) return { logs: [], executor: 'claudecode' };
  try {
    return JSON.parse(decodeURIComponent(h)) as MountData;
  } catch {
    return { logs: [], executor: 'claudecode' };
  }
}

const data = readDataFromHash();
const logs = data.logs;
const executor = data.executor;

try {
  const container = document.createElement('div');
  container.id = 'test-command-panel';
  container.style.padding = '20px';
  container.style.background = '#f5f5f5';
  container.style.maxWidth = '900px';
  container.style.margin = '0 auto';
  document.body.appendChild(container);

  const root = createRoot(container);
  root.render(React.createElement(CommandPanel, { logs, executor }));
  // 等 React 提交一帧
  setTimeout(() => { w.__renderDone = true; }, 500);
} catch (e) {
  w.__renderError = (e as Error).message;
  w.__renderDone = true;
}
