// 快速开始 5 步流程图：横向流程图 + 完成状态判断 + 跳转入口。
// 形态参考 MemorialBoard 的横向流程图风格。
// 完成判断走 useConceptCounts 的 quickStart 字段（已并行拉取）。

import { useCallback } from 'react';
import { Typography, Spin, Tag } from 'antd';
import { CheckCircleFilled, CiCircleOutlined, ArrowRightOutlined } from '@ant-design/icons';
import { useViewState } from '@/hooks/useViewState';
import { useConceptCounts } from '@/hooks/useConceptCounts';
import { QUICK_START_STEPS, type QuickStartStep } from '@/components/onboarding/concepts';

const { Text, Title } = Typography;

interface QuickStartFlowProps {
  /** 工作空间 id，用于完成状态判断。 */
  workspaceId: number | null;
}

/** 单步节点：圆形 + 标题 + 状态图标。 */
function StepNode({
  step,
  done,
  onGoto,
}: {
  step: QuickStartStep;
  done: boolean | null | undefined;
  onGoto: () => void;
}) {
  // done = null 表示拉取失败/未拉取，渲染灰色问号兜底。
  // done = true 绿色 ✓，done = false 灰色 ○。
  const statusIcon =
    done === true ? (
      <CheckCircleFilled style={{ fontSize: 24, color: '#52c41a' }} />
    ) : done === false ? (
      <CiCircleOutlined style={{ fontSize: 24, color: '#bfbfbf' }} />
    ) : (
      <CiCircleOutlined style={{ fontSize: 24, color: '#bfbfbf' }} />
    );

  return (
    <div
      onClick={onGoto}
      data-testid={`onboarding-flow-node-${step.index}`}
      style={{
        // 节点容器：垂直布局，圆形 + 标题。
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: 8,
        cursor: 'pointer',
        // 固定宽度避免横向布局时节点间距不均。
        width: 120,
        transition: 'transform 0.2s ease',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.transform = 'translateY(-2px)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.transform = 'translateY(0)';
      }}
    >
      {/* 状态圆形图标 */}
      {statusIcon}
      {/* 步骤序号小标签 */}
      <Tag
        color={done === true ? 'success' : 'default'}
        style={{ margin: 0, fontSize: 11 }}
      >
        步骤 {step.index}
      </Tag>
      {/* 步骤标题 */}
      <Text
        strong
        style={{ fontSize: 13, textAlign: 'center', lineHeight: 1.4 }}
      >
        {step.title}
      </Text>
    </div>
  );
}

/** 单条连线：横向箭头，暗示流程方向。 */
function StepConnector() {
  return (
    <div
      style={{
        // 横向连线：与节点中线对齐。
        display: 'flex',
        alignItems: 'center',
        // flex:1 让连线占据节点间的剩余空间，自适应总宽度。
        flex: 1,
        height: 24,
      }}
      aria-hidden="true"
    >
      {/* 横线 + 箭头，用 SVG 避免背景图定位问题。 */}
      <svg viewBox="0 0 48 24" width="48" height="24" style={{ display: 'block' }}>
        <line x1="0" y1="12" x2="40" y2="12" stroke="#d9d9d9" strokeWidth="2" />
        <path d="M 36 8 L 44 12 L 36 16 Z" fill="#d9d9d9" />
      </svg>
    </div>
  );
}

/**
 * 快速开始 5 步流程图。
 *
 * 整体处理思路：
 * 1. 横向 flex 布局：节点 + 连线 + 节点 + ... + 节点。
 * 2. 完成状态走 useConceptCounts.quickStart（并行拉取，与卡片网格共享）。
 * 3. 点击节点 → pushUrl 跳转到对应操作页面。
 * 4. loading 态：骨架屏占位，避免首屏空闪。
 */
export function QuickStartFlow({ workspaceId }: QuickStartFlowProps) {
  const { quickStart, loading } = useConceptCounts(workspaceId);
  const { pushUrl } = useViewState();

  // 跳转到对应操作页面。
  // useCallback 避免每次 render 重建函数。
  const handleGoto = useCallback(
    (target: QuickStartStep['navTarget']) => {
      pushUrl(target, {});
    },
    [pushUrl],
  );

  // loading 态且 quickStart 未就绪：骨架屏占位。
  if (loading && !quickStart) {
    return (
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 24,
          padding: '32px 0',
        }}
        data-testid="onboarding-flow-loading"
      >
        {QUICK_START_STEPS.map((step) => (
          <div
            key={step.index}
            style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              gap: 8,
              width: 120,
            }}
          >
            <Spin />
            <Tag style={{ margin: 0, fontSize: 11 }}>步骤 {step.index}</Tag>
            <Text type="secondary" style={{ fontSize: 13 }}>
              {step.title}
            </Text>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div
      style={{
        // 整体容器：横向流程图 + 上下间距。
        padding: '32px 16px',
        // 背景轻灰，与 Hero 区区分。
        background: 'var(--color-fill-quaternary, #f5f5f5)',
        borderRadius: 'var(--radius-md, 8px)',
        margin: '24px 0',
      }}
      data-testid="onboarding-flow"
    >
      {/* 头部说明 */}
      <Title level={4} style={{ marginTop: 0, marginBottom: 4 }}>
        5 步快速开始
      </Title>
      <Text type="secondary" style={{ fontSize: 13, display: 'block', marginBottom: 24 }}>
        按顺序完成下面 5 步，即可跑通你的第一个 AI 任务。已完成的步骤会打勾。
      </Text>

      {/* 横向流程图：节点 + 连线交替 */}
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          justifyContent: 'center',
          gap: 0,
        }}
      >
        {QUICK_START_STEPS.map((step, i) => (
          // key 用 step.index 避免连线和节点都取 i 造成 key 冲突。
          <div
            key={`step-${step.index}`}
            style={{
              display: 'flex',
              alignItems: 'center',
              // 最后一个节点不渲染连线，所以不 flex:1。
              flex: i === QUICK_START_STEPS.length - 1 ? '0 0 auto' : '1 1 auto',
            }}
          >
            <StepNode
              step={step}
              done={quickStart?.[step.index]}
              onGoto={() => handleGoto(step.navTarget)}
            />
            {/* 非最后一步渲染连线 */}
            {i < QUICK_START_STEPS.length - 1 && <StepConnector />}
          </div>
        ))}
      </div>

      {/* 底部提示：完成后下一步去向 */}
      <div style={{ marginTop: 24, textAlign: 'center' }}>
        <Text type="secondary" style={{ fontSize: 12 }}>
          <ArrowRightOutlined style={{ marginRight: 4 }} />
          完成后可去「仪表盘」查看整体运行情况，或在「看板」监控执行细节
        </Text>
      </div>
    </div>
  );
}
