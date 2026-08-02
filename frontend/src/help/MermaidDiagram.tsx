// 单个 mermaid 图的渲染组件。
//
// 设计要点：
// 1. mermaid 包体积较大（~2MB），用动态 import 懒加载，首次渲染时才拉取。
// 2. mermaid.initialize 只在主题切换时调用一次，避免重复初始化。
// 3. securityLevel: 'strict' 禁止 mermaid 源码里嵌入 HTML/script，防止 XSS。
// 4. 渲染失败（语法错误）时显示空，不打断整个文档。
// 5. mermaid.render 返回的是 mermaid 自己 sanitize 过的 svg，可安全用 dangerouslySetInnerHTML 注入。

import { useEffect, useRef, useState } from 'react';
import { useTheme } from '@/hooks/useTheme';

interface MermaidDiagramProps {
  /** mermaid 源码（不含 ```mermaid 围栏）。 */
  chart: string;
}

/** Mermaid 模块的类型（动态 import 后的形状）。 */
type MermaidModule = typeof import('mermaid');

/** 单例缓存：mermaid 模块只加载一次。 */
let mermaidModulePromise: Promise<MermaidModule> | null = null;

/**
 * 懒加载 mermaid 模块。
 *
 * @returns mermaid 模块
 */
function loadMermaid(): Promise<MermaidModule> {
  // 已发起加载则复用 promise，避免重复 import
  if (!mermaidModulePromise) {
    mermaidModulePromise = import('mermaid');
  }
  return mermaidModulePromise;
}

/** 单个 mermaid 图的渲染组件。 */
export function MermaidDiagram({ chart }: MermaidDiagramProps) {
  const { themeMode } = useTheme();
  // 渲染后的 svg 字符串，空串表示尚未渲染或渲染失败
  const [svg, setSvg] = useState('');
  // 用于 mermaid.render 的唯一 id，避免多图 id 冲突
  const idRef = useRef(`mmd-${Math.random().toString(36).slice(2)}`);

  // 主题切换时重新初始化 mermaid（theme 参数不同）
  useEffect(() => {
    let cancelled = false;
    loadMermaid().then(mermaid => {
      if (cancelled) return;
      // strict 模式禁止 mermaid 源码里嵌入 HTML/script，防止 XSS
      mermaid.default.initialize({
        startOnLoad: false,
        theme: themeMode === 'dark' ? 'dark' : 'default',
        securityLevel: 'strict',
      });
      // 初始化后立即触发一次渲染，确保主题切换后图也更新
      renderChart(mermaid);
    });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [themeMode]);

  // chart 变更时重新渲染
  useEffect(() => {
    let cancelled = false;
    loadMermaid().then(mermaid => {
      if (cancelled) return;
      renderChart(mermaid);
    });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chart]);

  /**
   * 用当前 mermaid 模块渲染 chart，成功后 setSvg。
   *
   * @param mermaid 已加载的 mermaid 模块
   */
  function renderChart(mermaid: MermaidModule): void {
    const id = idRef.current;
    mermaid.default.render(id, chart)
      .then(({ svg }) => setSvg(svg))
      .catch(() => setSvg('')); // 语法错误等失败时显示空
  }

  return (
    <div
      className="help-mermaid"
      // mermaid render 后注入的 svg 由 .help-mermaid svg 的 CSS 约束尺寸，
      // 这里仅提供容器；dangerouslySetInnerHTML 的内容是 mermaid sanitize 过的 svg。
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
