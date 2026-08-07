import { createContext, useContext, useState, useLayoutEffect, useCallback, useMemo, type ReactNode } from 'react';
import type { ThemeConfig } from 'antd';
import type { ThemeMode } from '@/themes';
import { themeMap } from '@/themes';

interface ThemeContextValue {
  themeMode: ThemeMode;
  themeConfig: ThemeConfig;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const STORAGE_KEY = 'app_theme';

function getInitialTheme(): ThemeMode {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'dark' || saved === 'light') return saved;
  } catch {}
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [themeMode, setThemeMode] = useState<ThemeMode>(getInitialTheme);

  useLayoutEffect(() => {
    document.documentElement.setAttribute('data-theme', themeMode);
  }, [themeMode]);

  useLayoutEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, themeMode);
    } catch {}
  }, [themeMode]);

  // 091：toggleTheme 与 value 记忆化，避免 Provider 每次渲染都产出新对象、
  // 拖着所有 useTheme 消费者一起重渲染（即便 themeMode 未变）。
  const toggleTheme = useCallback(() => {
    setThemeMode(prev => (prev === 'light' ? 'dark' : 'light'));
  }, []);

  const themeConfig = themeMap[themeMode];

  const value = useMemo<ThemeContextValue>(
    () => ({ themeMode, themeConfig, toggleTheme }),
    [themeMode, themeConfig, toggleTheme],
  );

  return (
    <ThemeContext.Provider value={value}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error('useTheme must be used within ThemeProvider');
  return ctx;
}
