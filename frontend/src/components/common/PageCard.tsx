import type { ReactNode, CSSProperties } from 'react';
import { Button } from 'antd';
import { ArrowLeftOutlined } from '@ant-design/icons';

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
 * @param titleSuffix - 标题文本后的附加元素（如折叠按钮），位于标题栏左侧区域
 * @param extra    - 标题栏右侧的操作按钮区域
 * @param onBack   - 062：传入后在 extra 区最右端渲染统一返回按钮（固定右上角锚点）
 * @param backLabel - 062：返回按钮文案，默认「返回列表」；返回目标非列表时传「返回」
 * @param children - 页面内容（渲染在横线下方）
 * @param showHeader - 是否显示顶部标题栏，默认为 true
 * @param className - 自定义类名
 * @param style - 自定义样式
 * @param contentClassName - 内容区域自定义类名
 * @param contentStyle - 内容区域自定义样式
 */
export function PageCard({
  icon,
  title,
  titleSuffix,
  extra,
  onBack,
  backLabel,
  children,
  showHeader = true,
  className,
  style,
  contentClassName,
  contentStyle,
}: {
  icon?: ReactNode;
  title?: ReactNode;
  titleSuffix?: ReactNode;
  extra?: ReactNode;
  onBack?: () => void;
  backLabel?: string;
  children: ReactNode;
  showHeader?: boolean;
  className?: string;
  style?: CSSProperties;
  contentClassName?: string;
  contentStyle?: CSSProperties;
}) {
  // 062：返回按钮统一由 PageCard 渲染，样式/位置全站一致；
  // 放在 extra 内容之后，保证它永远位于页头最右端（操作按钮数量变化不影响其锚点位置）。
  const backButton = onBack ? (
    <Button size="small" type="text" icon={<ArrowLeftOutlined />} onClick={onBack}>
      {backLabel ?? '返回列表'}
    </Button>
  ) : null;
  // extra 与 onBack 同时为空才不渲染右侧容器，保持无按钮页面的现状布局。
  const headerExtra = extra != null || backButton != null ? (
    <div className="ntd-page-card-extra">
      {extra}
      {backButton}
    </div>
  ) : null;

  return (
    <div className={`ntd-page-card ${className || ''}`} style={style}>
      {showHeader && (
        <>
          {/* 顶部标题栏：图标 + 标题 + 操作按钮 */}
          <div className="ntd-page-card-header">
            <div className="ntd-page-card-title">
              {icon && <span className="ntd-page-card-icon">{icon}</span>}
              {title && <span className="ntd-page-card-title-text">{title}</span>}
              {titleSuffix && <span className="ntd-page-card-title-suffix">{titleSuffix}</span>}
            </div>
            {headerExtra}
          </div>
          {/* 横线分隔 */}
          <div className="ntd-page-card-divider" />
        </>
      )}
      {/* 内容区域 */}
      <div className={`ntd-page-card-content ${contentClassName || ''}`} style={contentStyle}>
        {children}
      </div>
    </div>
  );
}
