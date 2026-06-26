import type { ReactNode, CSSProperties } from 'react';

/**
 * 右侧页面卡片容器。
 *
 * 为 Dashboard、看板、配置等独立页面提供统一的外观框架：
 * - 顶部区域：左侧图标+标题，右侧操作按钮
 * - 顶部使用圆角（--radius-lg）
 * - 标题区与内容区以横线分隔
 * - 内容区自适应填充
 *
 * @param icon     - 页面标题前的图标
 * @param title    - 页面标题文本
 * @param extra    - 标题栏右侧的操作按钮区域
 * @param children - 页面内容（渲染在横线下方）
 * @param className - 自定义类名
 * @param style - 自定义样式
 * @param contentClassName - 内容区域自定义类名
 * @param contentStyle - 内容区域自定义样式
 */
export function PageCard({
  icon,
  title,
  extra,
  children,
  className,
  style,
  contentClassName,
  contentStyle,
}: {
  icon?: ReactNode;
  title?: ReactNode;
  extra?: ReactNode;
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
  contentClassName?: string;
  contentStyle?: CSSProperties;
}) {
  return (
    <div className={`ntd-page-card ${className || ''}`} style={style}>
      {/* 顶部标题栏：图标 + 标题 + 操作按钮 */}
      <div className="ntd-page-card-header">
        <div className="ntd-page-card-title">
          {icon && <span className="ntd-page-card-icon">{icon}</span>}
          {title && <span className="ntd-page-card-title-text">{title}</span>}
        </div>
        {extra && <div className="ntd-page-card-extra">{extra}</div>}
      </div>
      {/* 横线分隔 */}
      <div className="ntd-page-card-divider" />
      {/* 内容区域 */}
      <div className={`ntd-page-card-content ${contentClassName || ''}`} style={contentStyle}>
        {children}
      </div>
    </div>
  );
}
