// 单个 mermaid 图的渲染组件（基于 merslim）。
//
// 设计要点：
// 1. 用 merslim 替代 mermaid：包体积 ~160KB（mermaid 的 1/20），纯 React/SVG 组件，
//    渲染可控，避免 mermaid SVG 等比缩放导致横向 flowchart 被压扁、字小到看不清。
// 2. merslim 语法兼容 mermaid 子集（flowchart/stateDiagram/classDiagram 等），
//    现有 md 无需改动。
// 3. bootstrapDiagramRenderers 全局注册一次，后续 <DiagramRenderer /> 直接用。
// 4. 主题适配：通过 dark prop 传给 DiagramRenderer，merslim 内部切换配色。
// 5. 渲染失败（语法错误）时 onError 兜底显示错误信息，不打断整个文档。

import { useEffect, useRef, useState } from 'react';
import {
  DiagramRenderer,
  bootstrapDiagramRenderers,
  type RendererHandle,
} from 'merslim';
import { useTheme } from '@/hooks/useTheme';

// 全局注册一次：merslim 的渲染器按图表类型懒加载，
// bootstrapDiagramRenderers 注册全部 14 种原生渲染器。
// 用标志位避免 React StrictMode 双触发重复注册。
let bootstrapped = false;
function ensureBootstrap(): void {
  if (bootstrapped) return;
  bootstrapDiagramRenderers();
  bootstrapped = true;
}

interface MermaidDiagramProps {
  /** mermaid 源码（不含 ```mermaid 围栏）。 */
  chart: string;
}

/**
 * 单个 mermaid 图的渲染组件。
 *
 * 用 merslim 的 DiagramRenderer 渲染，传入完整 mermaid 围栏语法源码。
 * merslim 内部 detectDiagramType 自动识别图表类型并路由到对应渲染器。
 *
 * @param chart mermaid 源码
 */
export function MermaidDiagram({ chart }: MermaidDiagramProps) {
  const { themeMode } = useTheme();
  // 渲染错误信息，空串表示无错误
  const [error, setError] = useState('');
  // RendererHandle ref，供 merslim 内部导出 SVG 用
  const handleRef = useRef<RendererHandle | null>(null);

  // 首次挂载时注册 merslim 渲染器
  useEffect(() => {
    ensureBootstrap();
  }, []);

  // chart 变更时清空错误状态，触发重新渲染
  useEffect(() => {
    setError('');
  }, [chart]);

  // 渲染失败兜底：显示错误提示，不打断整个文档
  if (error) {
    return (
      <div className="help-mermaid help-mermaid-error">
        {error}
      </div>
    );
  }

  // dark prop：themeMode 为 dark 时传 true，merslim 内部切换深色配色
  const isDark = themeMode === 'dark';

  return (
    <div className="help-mermaid" data-theme={themeMode}>
      <DiagramRenderer
        source={chart}
        dark={isDark}
        handleRef={handleRef}
        onError={setError}
      />
    </div>
  );
}
