/**
 * issue #652 — 浏览器侧 mount 脚本
 *
 * Playwright 加载 issue-652-mount.html，Vite dev server 把这个 .ts 文件
 * 当成 module 解析，import map / bare specifier 自动注入。
 *
 * 流程：
 * 1) 从 URL hash 读 result / status / recordId / showTitle（测试代码通过 hash 注入）
 * 2) createRoot 挂 CollapsibleConclusion 到 #test-collapsible-conclusion
 * 3) 写 window.__renderDone = true，让 Playwright 收尾
 */
import React from 'react';
import { createRoot } from 'react-dom/client';
import { CollapsibleConclusion } from '../src/components/todo-detail/CollapsibleConclusion.tsx';

interface MountData {
  result: string;
  status: string;
  recordId: number | string | null;
  showTitle: boolean;
}

interface MountWindow extends Window {
  __renderDone?: boolean;
  __renderError?: string;
}

const w = window as MountWindow;

function readDataFromHash(): MountData {
  const h = window.location.hash.replace(/^#/, '');
  if (!h) {
    return { result: '默认结果文本', status: 'success', recordId: null, showTitle: false };
  }
  try {
    return JSON.parse(decodeURIComponent(h)) as MountData;
  } catch {
    return { result: '默认结果文本', status: 'success', recordId: null, showTitle: false };
  }
}

const data = readDataFromHash();

try {
  const container = document.createElement('div');
  container.id = 'test-collapsible-conclusion';
  container.style.padding = '20px';
  container.style.background = '#f5f5f5';
  container.style.maxWidth = '900px';
  container.style.margin = '0 auto';
  document.body.appendChild(container);

  const root = createRoot(container);
  // 构造一个空的 messageApi，避免在 Playwright headless 环境下 antd 静态 message 实例冲突
  const messageApi = {
    success: (msg: string) => console.log('[mock message success]', msg),
    error: (msg: string) => console.log('[mock message error]', msg),
  };
  root.render(React.createElement(CollapsibleConclusion, {
    result: data.result,
    status: data.status,
    messageApi,
    showTitle: data.showTitle,
    recordId: data.recordId ?? undefined,
  }));
  setTimeout(() => { w.__renderDone = true; }, 500);
} catch (e) {
  w.__renderError = (e as Error).message;
  w.__renderDone = true;
}
