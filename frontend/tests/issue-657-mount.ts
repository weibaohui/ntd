/**
 * issue #657 — 浏览器侧 mount 脚本
 *
 * Playwright 加载 issue-657-mount.html，Vite dev server 把这个 .ts 文件
 * 当成 module 解析，bare specifier 自动注入。
 *
 * 从 URL hash 读 component / logs / record，渲染对应组件（NarrowLogView /
 * ContinuationLogView / ContinuationLogsLoader）到 #test-target，便于
 * Playwright 在窄屏下校验「命令」视图分支是否生效。
 *
 * PR #657 复查 C1 修复：Harness 用 useState 跟踪 viewMode，让 mount harness
 * 真的把 Segmented 的 onChange 反馈回受测组件，从而能验证"切到 chat/command
 * 自动展开"的 useEffect 同步逻辑。
 */
import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { NarrowLogView } from '../src/components/todo-detail/NarrowLogView.tsx';
import { ContinuationLogView } from '../src/components/todo-detail/ContinuationLogView.tsx';
import { ContinuationLogsLoader } from '../src/components/todo-detail/ContinuationLogsLoader.tsx';
import type { LogEntry, ExecutionRecord } from '../src/types/index.ts';

type ComponentKind = 'NarrowLogView' | 'ContinuationLogView' | 'ContinuationLogsLoader';
type ViewMode = 'log' | 'chat' | 'command';

interface MountData {
  component: ComponentKind;
  logs: LogEntry[];
  executor: string;
  viewMode: ViewMode;
  recordId: number;
}

interface MountWindow extends Window {
  __renderDone?: boolean;
  __renderError?: string;
  __viewModeChanges?: ViewMode[];
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

const initialData = readDataFromHash();

const sampleRecord: ExecutionRecord = {
  id: initialData.recordId,
  todoId: 1,
  executor: initialData.executor,
  status: 'success',
} as unknown as ExecutionRecord;

/**
 * 包装受测组件：让 viewMode 由 React state 控制，模拟真实业务下"用户点击 Segmented 切视图"的链路。
 * 没有这个 Harness，onViewModeChange 是 no-op，useEffect 同步逻辑根本测不到。
 */
function Harness() {
  const [viewMode, setViewMode] = useState<ViewMode>(initialData.viewMode);
  const handleChange = (m: ViewMode) => {
    w.__viewModeChanges = [...(w.__viewModeChanges || []), m];
    setViewMode(m);
  };
  if (initialData.component === 'NarrowLogView') {
    return React.createElement(NarrowLogView, {
      record: sampleRecord,
      isRunning: false,
      displayLogs: initialData.logs,
      liveLogs: null,
      viewMode,
      onRefresh: () => {},
      onViewModeChange: handleChange,
    });
  } else if (initialData.component === 'ContinuationLogView') {
    return React.createElement(ContinuationLogView, {
      record: sampleRecord,
      logs: initialData.logs,
      isRunning: false,
      viewMode,
      onRefresh: () => {},
      onViewModeChange: handleChange,
    });
  }
  // ContinuationLogsLoader：直接传 logs 跳过懒加载，方便静态 mount 渲染命令面板。
  return React.createElement(ContinuationLogsLoader, {
    record: sampleRecord,
    logs: initialData.logs,
    viewMode,
    onRefresh: () => {},
    onViewModeChange: handleChange,
  });
}

try {
  const container = document.createElement('div');
  container.id = 'test-target';
  container.style.padding = '12px';
  container.style.background = '#fafafa';
  container.style.maxWidth = '420px';
  container.style.margin = '0 auto';
  document.body.appendChild(container);

  const root = createRoot(container);
  root.render(React.createElement(Harness));
  // 等 React 提交一帧
  setTimeout(() => { w.__renderDone = true; }, 500);
} catch (e) {
  w.__renderError = (e as Error).message;
  w.__renderDone = true;
}
