// 概念卡片网格：6 张差异化可视化卡片 + 当前数量徽标。
// 响应式 grid（auto-fill minmax(280px,1fr)），与 TodoCenterCardView 风格一致。
// hover：阴影增强 + translateY(-2px)，transition 200ms。
// 点击：平滑滚动到对应详细说明区锚点。

import { useCallback } from 'react';
import { Typography, Spin } from 'antd';
import {
  BuildOutlined,
  MacCommandOutlined,
  RetweetOutlined,
  RocketOutlined,
  TeamOutlined,
  ThunderboltOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons';
import { useConceptCounts } from '@/hooks/useConceptCounts';
import { CONCEPTS, type ConceptNode } from '@/components/onboarding/concepts';

const { Text, Title } = Typography;

interface ConceptCardGridProps {
  /** 工作空间 id，用于拉数量。 */
  workspaceId: number | null;
}

/** 滚动到指定概念详细说明区。 */
function scrollToConcept(id: ConceptNode['id']) {
  // 用原生 scrollIntoView + smooth behavior，不引入新依赖。
  const el = document.getElementById(`concept-${id}`);
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
}

/**
 * 差异化可视化隐喻。
 *
 * 6 个概念各用不同的迷你图形，而非统一图标，
 * 帮助用户从视觉上区分「抽象层级」：
 *   工艺（3 圆点连线）/ 环路（循环箭头）/ 事项（卡片堆叠）
 *   任务（闪电 + 4 色状态点）/ 执行器（终端）/ 专家（人形）
 */
function ConceptCardVisual({ id }: { id: ConceptNode['id'] }) {
  // 统一尺寸 48x48，内部图形差异化。
  const wrapperStyle: React.CSSProperties = {
    width: 48,
    height: 48,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 12,
  };
  switch (id) {
    case 'process':
      // 工艺：3 圆点连线，暗示「阶段→环节」链式结构。
      return (
        <div style={wrapperStyle}>
          <svg viewBox="0 0 48 48" width="48" height="48">
            <line x1="8" y1="24" x2="18" y2="24" stroke="#1677ff" strokeWidth="2" />
            <line x1="30" y1="24" x2="40" y2="24" stroke="#1677ff" strokeWidth="2" />
            <circle cx="8" cy="24" r="4" fill="#1677ff" />
            <circle cx="24" cy="24" r="4" fill="#1677ff" />
            <circle cx="40" cy="24" r="4" fill="#1677ff" />
          </svg>
        </div>
      );
    case 'loop':
      // 环路：循环箭头，暗示可重复执行。
      return (
        <div style={wrapperStyle}>
          <svg viewBox="0 0 48 48" width="48" height="48">
            <path
              d="M 12 24 A 12 12 0 1 1 36 24"
              stroke="#722ed1"
              strokeWidth="2"
              fill="none"
            />
            <path d="M 36 24 L 30 20 L 30 28 Z" fill="#722ed1" />
          </svg>
        </div>
      );
    case 'todo':
      // 事项：卡片堆叠，暗示执行单元。
      return (
        <div style={wrapperStyle}>
          <svg viewBox="0 0 48 48" width="48" height="48">
            <rect x="8" y="10" width="22" height="14" rx="2" fill="#52c41a" opacity="0.4" />
            <rect x="14" y="14" width="22" height="14" rx="2" fill="#52c41a" opacity="0.7" />
            <rect x="20" y="18" width="22" height="14" rx="2" fill="#52c41a" />
          </svg>
        </div>
      );
    case 'task':
      // 任务：闪电 + 4 色状态点，暗示触发意图 + 状态流转。
      return (
        <div style={wrapperStyle}>
          <svg viewBox="0 0 48 48" width="48" height="48">
            <path d="M 22 8 L 14 24 L 22 24 L 18 40 L 34 18 L 24 18 Z" fill="#fa8c16" />
            <circle cx="8" cy="40" r="3" fill="#bfbfbf" />
            <circle cx="16" cy="44" r="3" fill="#1677ff" />
            <circle cx="24" cy="40" r="3" fill="#52c41a" />
            <circle cx="32" cy="44" r="3" fill="#ff4d4f" />
          </svg>
        </div>
      );
    case 'executor':
      // 执行器：终端图标，暗示 CLI 工具。
      return (
        <div style={wrapperStyle}>
          <MacCommandOutlined style={{ fontSize: 36, color: '#13c2c2' }} />
        </div>
      );
    case 'expert':
      // 专家：人形图标，暗示人格化配置。
      return (
        <div style={wrapperStyle}>
          <TeamOutlined style={{ fontSize: 36, color: '#eb2f96' }} />
        </div>
      );
    default:
      return null;
  }
}

/** 单张概念卡片。 */
function ConceptCard({
  concept,
  count,
  onClick,
}: {
  concept: ConceptNode;
  count: number | null | undefined;
  onClick: () => void;
}) {
  return (
    <div
      onClick={onClick}
      data-testid={`onboarding-card-${concept.id}`}
      style={{
        // 卡片基础样式：白底、圆角、轻阴影，与 TodoCard 一致。
        background: 'var(--color-bg-card, #fff)',
        borderRadius: 'var(--radius-md, 8px)',
        padding: 20,
        boxShadow: '0 1px 2px rgba(0, 0, 0, 0.06)',
        cursor: 'pointer',
        transition: 'box-shadow 0.2s ease, transform 0.2s ease',
        position: 'relative',
        minHeight: 160,
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.boxShadow = '0 4px 8px rgba(0, 0, 0, 0.1)';
        e.currentTarget.style.transform = 'translateY(-2px)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.boxShadow = '0 1px 2px rgba(0, 0, 0, 0.06)';
        e.currentTarget.style.transform = 'translateY(0)';
      }}
    >
      {/* 差异化可视化 */}
      <ConceptCardVisual id={concept.id} />
      {/* 标题 */}
      <Text strong style={{ fontSize: 16, display: 'block', marginBottom: 4 }}>
        {concept.label}
      </Text>
      {/* 一句话定义 */}
      <Text type="secondary" style={{ fontSize: 13, lineHeight: 1.5 }}>
        {concept.oneLiner}
      </Text>
      {/* 数量徽标：拉取失败（null）时不显示 */}
      {count != null && (
        <span
          style={{
            position: 'absolute',
            top: 12,
            right: 12,
            background: 'var(--color-primary, #1677ff)',
            color: '#fff',
            borderRadius: 10,
            padding: '2px 8px',
            fontSize: 12,
            fontWeight: 500,
          }}
          data-testid={`onboarding-card-badge-${concept.id}`}
        >
          {count}
        </span>
      )}
    </div>
  );
}

/**
 * 概念卡片网格。
 *
 * 整体处理思路：
 * 1. useConceptCounts 拉取 6 个概念当前数量。
 * 2. 响应式 grid 布局：auto-fill + minmax(280px, 1fr) 自适应列数。
 * 3. 点击卡片 → scrollIntoView 平滑滚动到对应详细说明区。
 */
export function ConceptCardGrid({ workspaceId }: ConceptCardGridProps) {
  const { counts, loading } = useConceptCounts(workspaceId);

  // 点击卡片 → 滚动到对应详细说明区。
  // useCallback 避免每次 render 重建函数。
  const handleClick = useCallback((id: ConceptNode['id']) => {
    scrollToConcept(id);
  }, []);

  // loading 态：骨架屏占位。
  if (loading && !counts) {
    return (
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
          gap: 16,
          margin: '24px 0',
        }}
      >
        {CONCEPTS.map((c) => (
          <div
            key={c.id}
            style={{
              background: 'var(--color-bg-card, #fff)',
              borderRadius: 'var(--radius-md, 8px)',
              padding: 20,
              boxShadow: '0 1px 2px rgba(0, 0, 0, 0.06)',
              minHeight: 160,
            }}
          >
            <Spin />
          </div>
        ))}
      </div>
    );
  }

  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
        gap: 16,
        margin: '24px 0',
      }}
      data-testid="onboarding-card-grid"
    >
      {CONCEPTS.map((concept) => (
        <ConceptCard
          key={concept.id}
          concept={concept}
          count={counts?.[concept.id]}
          onClick={() => handleClick(concept.id)}
        />
      ))}
    </div>
  );
}

// 保留 import 引用占位（避免 tsc unused 警告，下方未直接用到的图标）。
// 这些图标在 concepts.tsx 内已用过，这里不出现在 JSX 里即可。
void BuildOutlined;
void RetweetOutlined;
void RocketOutlined;
void TeamOutlined;
void ThunderboltOutlined;
void UnorderedListOutlined;
void Title;
