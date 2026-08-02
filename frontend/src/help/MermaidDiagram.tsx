// 单个 mermaid 图的渲染组件。
//
// 设计要点：
// 1. mermaid 包体积较大（~2MB），用动态 import 懒加载，首次渲染时才拉取。
// 2. mermaid.initialize 只在主题切换时调用一次，避免重复初始化。
// 3. securityLevel: 'strict' 禁止 mermaid 源码里嵌入 HTML/script，防止 XSS。
// 4. 渲染失败（语法错误）时显示空，不打断整个文档。
// 5. mermaid.render 返回的是 mermaid 自己 sanitize 过的 svg，可安全用 dangerouslySetInnerHTML 注入。
// 6. SVG 自适应：mermaid 默认给 svg 打固定 width/height 像素值，会导致图按固定尺寸渲染、
//    宽度被 CSS 压缩时高度不跟随、比例失调。注入前规整属性——保留 viewBox（缺则从 width/height 派生）、
//    移除固定 width/height，让 svg 用 width:100% height:auto 真正按容器等比缩放；
//    容器 overflow:auto 兼底，图过大时出现滚动条。

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

/**
 * 规整 mermaid 渲染出的 svg 字串，使其支持容器自适应缩放。
 *
 * mermaid 默认给 svg 打固定 width/height 像素（如 width="800" height="400"），
 * 直接注入会让图按固定尺寸渲染，宽度被 CSS 压缩时高度不跟随、比例失调。
 *
 * 处理步骤：
 * 1. 解析 svg 根节点上的 width/height 属性（像素或百分比）。
 * 2. 若无 viewBox，用解析到的 width/height 派生 viewBox（保留原始比例坐标空间）。
 * 3. 移除 svg 根节点的 width/height/style.max-width/style.max-height 属性，
 *    让外层 CSS 的 width:100% height:auto 接管。
 * 4. 给 svg 加 preserveAspectRatio="xMidYMid meet"，确保居中等比缩放。
 *
 * 失败（解析不到 svg、属性异常）时原样返回，由 CSS 兜底兜底。
 *
 * @param svg mermaid render 返回的 svg 字串
 * @returns 规整后的 svg 字串
 */
function normalizeSvgForResponsive(svg: string): string {
  // 只处理根 svg 开标签；mermaid 输出无 DOCTYPE/外层注释干扰
  const svgOpenMatch = svg.match(/<svg\b[^>]*>/i);
  if (!svgOpenMatch) return svg;

  const svgOpen = svgOpenMatch[0];
  // 提取 width / height 属性值（mermaid 常给像素整数或带 px 单位，也可能给百分比）
  const widthMatch = svgOpen.match(/\swidth="([^"]+)"/i);
  const heightMatch = svgOpen.match(/\sheight="([^"]+)"/i);
  const viewBoxMatch = svgOpen.match(/\sviewBox="([^"]+)"/i);

  // 解析出数字 px 值，用于派生 viewBox；非数字（如百分比）则跳过派生
  const widthPx = widthMatch ? parseFloat(widthMatch[1]) : NaN;
  const heightPx = heightMatch ? parseFloat(heightMatch[1]) : NaN;

  // 组装新 svg 开标签：移除 width/height/style 中的 max-width/max-height，保留其他属性
  // 用正则去掉 width / height / style 整段，后面再按需补回 viewBox / preserveAspectRatio
  let cleaned = svgOpen
    .replace(/\swidth="[^"]*"/i, '')
    .replace(/\sheight="[^"]*"/i, '')
    // 只移 style 里的 max-width/max-height 声明，保留其他 style（如字体）
    .replace(/\sstyle="([^"]*)"/i, (_m, styleVal: string) => {
      const kept = styleVal
        .split(';')
        .map(s => s.trim())
        .filter(s => s && !/^max-width/i.test(s) && !/^max-height/i.test(s));
      return kept.length ? ` style="${kept.join('; ')}"` : '';
    });

  // 若原本无 viewBox 且能解析到有效宽高，派生 viewBox 保住比例坐标空间
  if (!viewBoxMatch && Number.isFinite(widthPx) && Number.isFinite(heightPx) && widthPx > 0 && heightPx > 0) {
    cleaned = cleaned.replace(/<svg\b/i, `<svg viewBox="0 0 ${widthPx} ${heightPx}"`);
  }

  // 加 preserveAspectRatio 保证居中等比缩放（已有则替换，无则追加）
  if (/\spreserveAspectRatio="/i.test(cleaned)) {
    cleaned = cleaned.replace(/\spreserveAspectRatio="[^"]*"/i, ' preserveAspectRatio="xMidYMid meet"');
  } else {
    cleaned = cleaned.replace(/<svg\b/i, '<svg preserveAspectRatio="xMidYMid meet"');
  }

  // 把规整后的开标签替换回原字串
  return svg.replace(svgOpenMatch[0], cleaned);
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
      // themeVariables 自定义美化：节点圆角、主色边、字体、间距，避免默认白底黑框的丑陋外观
      mermaid.default.initialize({
        startOnLoad: false,
        theme: themeMode === 'dark' ? 'dark' : 'base',
        themeVariables: themeMode === 'dark' ? {
          // 深色：节点深灰底 + 浅字 + 主色边
          primaryColor: '#1e293b',
          primaryTextColor: '#e2e8f0',
          primaryBorderColor: '#3b82f6',
          lineColor: '#64748b',
          secondaryColor: '#334155',
          tertiaryColor: '#0f172a',
          background: '#0f172a',
          mainBkg: '#1e293b',
          secondBkg: '#334155',
          textColor: '#e2e8f0',
          fontSize: '14px',
          fontFamily: 'system-ui, -apple-system, sans-serif',
        } : {
          // 浅色：节点白底 + 深字 + 主色边
          primaryColor: '#ffffff',
          primaryTextColor: '#1e293b',
          primaryBorderColor: '#3b82f6',
          lineColor: '#64748b',
          secondaryColor: '#f1f5f9',
          tertiaryColor: '#f8fafc',
          background: '#ffffff',
          mainBkg: '#ffffff',
          secondBkg: '#f1f5f9',
          textColor: '#1e293b',
          fontSize: '14px',
          fontFamily: 'system-ui, -apple-system, sans-serif',
        },
        securityLevel: 'strict',
        // 用 CSS 接管尺寸：让 mermaid 不再注入固定像素的 width/height，
        // 改由 normalizeSvgForResponsive + .help-mermaid svg CSS 自适应容器。
        flowchart: {
          useMaxWidth: false,
          // 节点圆角 + 间距美化
          curve: 'basis',
          padding: 12,
          nodeSpacing: 40,
          rankSpacing: 40,
        },
        sequence: { useMaxWidth: false },
        gantt: { useMaxWidth: false },
      });
      // 初始化后立即触发一次渲染，确保主题切换后图也更新
      renderChart(mermaid);
    });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [themeMode]);

  // chart 号新时重新渲染
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
   * 用当前 mermaid 模块渲染 chart，成功后规整 svg 并 setSvg。
   *
   * @param mermaid 已加载的 mermaid 模块
   */
  function renderChart(mermaid: MermaidModule): void {
    const id = idRef.current;
    mermaid.default.render(id, chart)
      .then(({ svg }) => setSvg(normalizeSvgForResponsive(svg)))
      .catch(() => setSvg('')); // 语法错误等失败时显示空
  }

  return (
    <div
      className="help-mermaid"
      // mermaid render 后注入的 svg 由 .help-mermaid svg 的 CSS 接管尺寸自适应，
      // 容器 overflow:auto 兜底，图过大时出现滚动条。
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
