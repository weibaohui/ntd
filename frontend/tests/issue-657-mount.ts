/**
 * issue #657 — 浏览器侧 mount 脚本
 *
 * Playwright 加载 issue-657-mount.html，Vite dev server 把这个 .ts 文件
 * 当成 module 解析，bare specifier 自动注入。
 *
 * 从 URL hash 读 component / logs / record，渲染对应组件（NarrowLogView /
 * ContinuationLogView / ContinuationLogsLoader）到 #test-target，便于
 * Playwright 在窄屏下校验「命令」视图分支是否生效。
 */
import React from 'react';
import { createRoot } from 'react-dom/client';
import { NarrowLogView } from '../src/components/todo-detail/NarrowLogView.tsx';
import { ContinuationLogView } from '../src/components/todo-detail/ContinuationLogView.tsx';
import { ContinuationLogsLoader } from '../src/components/todo-detail/ContinuationLogsLoader.tsx';
import type { LogEntry, ExecutionRecord } from '../src/types/index.ts';

type ComponentKind = 'NarrowLogView' | 'ContinuationLogView' | 'ContinuationLogsLoader';

interface MountData {
  component: ComponentKind;
  logs: LogEntry[];
  executor: string;
  viewMode: 'log' | 'chat' | 'command';
  recordId: number;
}

interface MountWindow extends Window {
  __renderDone?: boolean;
  __renderError?: string;
}

const w = window as MountWindow;

function readDataFromHash(): MountData {
  const h = window.location.hash.replace(/^#/, '');
  if (!h) {
    return { component: 'NarrowLogView', logs: [], executor: 'claudecode', viewMode: 'command', recordId: 1 };
  }
  try {
    return JSON.parse(decodeURIComponent(h)) as MountData;
  } catch {
    return { component: 'NarrowLogView', logs: [], executor: 'claudecode', viewMode: 'command', recordId: 1 };
  }
}

const data = readDataFromHash();

const sampleRecord: ExecutionRecord = {
  id: data.recordId,
  todoId: 1,
  executor: data.executor,
  status: 'success',
} as unknown as ExecutionRecord;

try {
  const container = document.createElement('div');
  container.id = 'test-target';
  container.style.padding = '12px';
  container.style.background = '#fafafa';
  container.style.maxWidth = '420px';
  container.style.margin = '0 auto';
  document.body.appendChild(container);

  let element: React.ReactElement;
  if (data.component === 'NarrowLogView') {
    element = React.createElement(NarrowLogView, {
      record: sampleRecord,
      isRunning: false,
      displayLogs: data.logs,
      liveLogs: null,
      viewMode: data.viewMode,
      onRefresh: () => {},
      onViewModeChange: () => {},
    });
  } else if (data.component === 'ContinuationLogView') {
    element = React.createElement(ContinuationLogView, {
      record: sampleRecord,
      logs: data.logs,
      isRunning: false,
      viewMode: data.viewMode,
      onRefresh: () => {},
      onViewModeChange: () => {},
    });
  } else {
    // 直接传入 logs：跳过懒加载让组件在无后端的静态 mount 下也能渲染命令视图。
    element = React.createElement(ContinuationLogsLoader, {
      record: sampleRecord,
      logs: data.logs,
      viewMode: data.viewMode,
      onRefresh: () => {},
      onViewModeChange: () => {},
    });
  }
  const root = createRoot(container);
  root.render(element);
  // 等 React 提交一帧
  setTimeout(() => { w.__renderDone = true; }, 500);
} catch (e) {
  w.__renderError = (e as Error).message;
  w.__renderDone = true;
}
