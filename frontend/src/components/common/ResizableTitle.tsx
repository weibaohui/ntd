/**
 * ResizableTitle — 可拖拽列宽的 antd Table 表头 cell 组件。
 *
 * 设计要点：
 * 1. 通过 antd Table 的 `components={{ header: { cell: ResizableTitle } }}` 注入；
 *    列的 width / onResize / onResizeEnd 由列定义上的 `onHeaderCell` 透传进来。
 * 2. 拖拽用 Pointer Events（非 Mouse Events），天然兼容触屏与鼠标。
 * 3. `setPointerCapture` 把后续 pointermove/pointerup 路由到手柄元素，
 *    即使指针快速移出表头也能持续追踪，避免"断线"。
 * 4. 拖拽中禁用 document.body 的 userSelect，防止文本被意外选中。
 * 5. 最小列宽 60px，防止用户把列拖成 0 宽导致内容消失。
 * 6. 拖拽过程只回调 onResize（高频更新 state），结束时回调 onResizeEnd
 *    （父级在此刻写 localStorage），避免拖拽期间高频写存储。
 *
 * 本组件不持有列宽 state，只通过回调通知父级 hook（useResizableColumns）。
 */

import React, { useRef } from 'react';

/** 最小列宽（px）：低于此值锁定，防止列被拖成不可见。 */
const MIN_WIDTH = 60;

export interface ResizableTitleProps {
  /** 当前列宽（由列定义 onHeaderCell 注入）。 */
  width?: number;
  /** 固定列标记（antd fixed: 'left'|'right'）：固定列已是 sticky 定位，
      不能再设 position:relative（inline 样式会覆盖 class 的 sticky）。 */
  fixed?: string | boolean;
  /** 拖拽中回调：实时新宽度。 */
  onResize?: (width: number) => void;
  /** 拖拽结束回调：最终宽度（父级在此持久化）。 */
  onResizeEnd?: (width: number) => void;
  /** antd 透传的子节点（列标题文本）。 */
  children?: React.ReactNode;
  /** antd 透传的其他 th 属性（className / style 等）。 */
  [key: string]: unknown;
}

/**
 * 可拖拽表头 cell。
 * 无 width 的列（操作列、选择框列等）退化为普通 th，不附加拖拽手柄。
 */
export const ResizableTitle: React.FC<ResizableTitleProps> = ({
  width,
  fixed,
  onResize,
  onResizeEnd,
  children,
  ...restProps
}) => {
  // 拖拽会话状态用 ref 存储：避免高频 pointermove 触发组件重渲染。
  // dragging 用独立布尔值而非 startX===0 判断，因为 clientX 合法值可以是 0（屏幕左边缘）。
  const draggingRef = useRef(false);
  const startXRef = useRef(0);
  const startWidthRef = useRef(0);
  const latestWidthRef = useRef(0);

  if (!width) {
    return <th {...restProps}>{children}</th>;
  }

  const handlePointerDown = (e: React.PointerEvent) => {
    // 只在主键（鼠标左键/触屏单指）触发，避免右键/中键误触。
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    draggingRef.current = true;
    startXRef.current = e.clientX;
    startWidthRef.current = width;
    latestWidthRef.current = width;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    // 拖拽中禁止文本选中，避免用户看到蓝色高亮块。
    document.body.style.userSelect = 'none';
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!draggingRef.current) return;
    const delta = e.clientX - startXRef.current;
    const newWidth = Math.max(MIN_WIDTH, startWidthRef.current + delta);
    latestWidthRef.current = newWidth;
    onResize?.(newWidth);
  };

  const handlePointerUp = (e: React.PointerEvent) => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    document.body.style.userSelect = '';
    // 只在宽度真正变化时才通知结束，避免纯点击（未拖动）触发无谓的持久化。
    if (latestWidthRef.current !== startWidthRef.current) {
      onResizeEnd?.(latestWidthRef.current);
    }
  };

  return (
    <th
      {...restProps}
      style={{
        ...((restProps.style as React.CSSProperties) ?? {}),
        width,
        // 固定列（fixed left/right）已由 antd class 设为 sticky，
        // 不能再覆盖为 relative；非固定列需 relative 作为拖拽手柄的定位父级。
        ...(fixed ? {} : { position: 'relative' as const }),
      }}
    >
      {children}
      {/* 拖拽手柄：表头右边缘 8px 热区；stopPropagation 防止触发列头排序切换。 */}
      <span
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        onClick={(e) => e.stopPropagation()}
        style={{
          position: 'absolute',
          right: 0,
          top: 0,
          bottom: 0,
          width: 8,
          cursor: 'col-resize',
          zIndex: 1,
        }}
      />
    </th>
  );
};
