import { createContext, useContext, useState, useLayoutEffect, type ReactNode } from 'react';

// 底部全局执行日志面板（ExecutionPanel）的显隐偏好，属于纯前端 UI 偏好，
// 走 localStorage 而非后端 config，模式与 useTheme 一致。
interface ConsolePanelContextValue {
  visible: boolean;
  setVisible: (next: boolean) => void;
}

const ConsolePanelContext = createContext<ConsolePanelContextValue | null>(null);

// 复用单一 key，设置页开关与 App 根布局都读写它，避免状态分裂。
const STORAGE_KEY = 'ntd_console_panel_visible';

// 默认开启：保留现有「任务运行时面板自动出现」的行为，避免存量用户升级后看不到执行日志。
function getInitialVisible(): boolean {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'true') return true;
    if (saved === 'false') return false;
  } catch {
    // localStorage 在隐私模式/禁用 cookie 时可能抛错，忽略后走默认值，不阻断渲染。
  }
  return true;
}

export function ConsolePanelProvider({ children }: { children: ReactNode }) {
  const [visible, setVisibleState] = useState<boolean>(getInitialVisible);

  // 写入 localStorage：刷新或下次进入仍能恢复，纯前端偏好无需落库。
  useLayoutEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, String(visible));
    } catch {
      // 同上：写入失败不致命，内存态仍生效，仅本次会话丢失持久化。
    }
  }, [visible]);

  const setVisible = (next: boolean) => setVisibleState(next);

  return (
    <ConsolePanelContext.Provider value={{ visible, setVisible }}>
      {children}
    </ConsolePanelContext.Provider>
  );
}

export function useConsolePanel() {
  const ctx = useContext(ConsolePanelContext);
  if (!ctx) throw new Error('useConsolePanel must be used within ConsolePanelProvider');
  return ctx;
}
