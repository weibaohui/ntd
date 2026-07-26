// 统一追溯面包屑组件。
// 用于环路详情、工艺详情、事项详情等页面，显性化"数据实例来源于何处"这条追溯链路。
//
// P2 设计：替代各页面当前的行内简单实现，提供一致的视觉语言与交互模式。

import { BuildOutlined } from '@ant-design/icons';

interface BreadcrumbSegment {
  /** 显示文本 */
  label: string;
  /** 可选的技术名（括号内小字展示） */
  techName?: string;
  /** 可选版本号 */
  version?: string;
  /** 点击回调；不传则不可点击 */
  onClick?: () => void;
}

interface TraceBreadcrumbProps {
  /** 标题前缀（如"来源工艺"、"所属任务"），不传则无前缀 */
  title?: string;
  /** 面包屑分段列表 */
  segments: BreadcrumbSegment[];
  /** 可选的自定义样式 */
  style?: React.CSSProperties;
}

export function TraceBreadcrumb({ title, segments, style }: TraceBreadcrumbProps) {
  if (segments.length === 0) return null;

  return (
    <div
      style={{
        display: 'flex', alignItems: 'center', gap: 4, flexWrap: 'wrap',
        marginBottom: 12, fontSize: 12,
        color: 'var(--color-text-secondary, #475569)',
        ...style,
      }}
    >
      <BuildOutlined style={{ color: 'var(--color-text-tertiary, #94a3b8)' }} />
      {title && <span>{title}：</span>}
      {segments.map((seg, i) => (
        <span key={i} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          {i > 0 && (
            <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', margin: '0 2px' }}>›</span>
          )}
          <span
            onClick={seg.onClick}
            style={{
              color: seg.onClick ? '#0891b2' : 'var(--color-text-primary, #0f172a)',
              fontWeight: seg.onClick ? 500 : 400,
              cursor: seg.onClick ? 'pointer' : 'default',
              textDecoration: seg.onClick ? 'underline' : 'none',
              textUnderlineOffset: 3,
            }}
          >
            {seg.label}
          </span>
          {seg.techName && (
            <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>({seg.techName})</span>
          )}
          {seg.version && (
            <span style={{
              padding: '0 6px', borderRadius: 8, fontSize: 11,
              background: 'var(--color-bg-subtle, #f1f5f9)',
              color: 'var(--color-text-tertiary, #64748b)',
            }}>v{seg.version}</span>
          )}
        </span>
      ))}
    </div>
  );
}
