import { useState, useEffect } from 'react';

export function useIsMobile(threshold = 768): boolean {
  // lazy 初始值直接读 window 宽度,避免首次渲染按「桌面」布局、useEffect 后再闪切到「移动」,
  // 消除 SSR/首屏抖动与测试竞态(playwright 点按移动端 label 时元素尚未切换)。
  const [isMobile, setIsMobile] = useState(
    () => typeof window !== 'undefined' && window.innerWidth < threshold,
  );
  useEffect(() => {
    const check = () => setIsMobile(window.innerWidth < threshold);
    check();
    window.addEventListener('resize', check);
    return () => window.removeEventListener('resize', check);
  }, [threshold]);
  return isMobile;
}
