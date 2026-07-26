// 工艺模板只读流程图节点卡片。
// 与 FlowStepNode（环路实例运行时样式）对应，但采用更静态、更文档化的视觉语言：
// - 无选中态
// - 无状态 dot（模板无运行时）
// - 显示 prompt 摘要、门禁数量、executor 标签
// - 顶部显示序号徽章（蓝灰调）

import {
  NODE_WIDTH, NODE_HEIGHT,
} from '@/components/loop-flow/flowConstants';
import { truncateText } from '@/components/loop-flow/useFlowLayout';
import type { AdaptedLink } from '@/components/process/processFlowAdapter';

export interface TemplateStepCardProps {
  /** 适配后的链接数据。 */
  link: AdaptedLink;
  /** SVG 左上角 x。 */
  x: number;
  /** SVG 左上角 y。 */
  y: number;
}

export function TemplateStepCard({ link, x, y }: TemplateStepCardProps) {
  const gateCount = link.link.gates?.length ?? 0;
  const promptSnippet = (link.link.prompt || '')
    .replace(/\n/g, ' ').trim().slice(0, 30);
  const executor = link.link.executor || link.link.expert || 'atomcode';

  return (
    <g>
      {/* 卡片背景 */}
      <rect
        x={x} y={y}
        width={NODE_WIDTH} height={NODE_HEIGHT}
        rx={6} ry={6}
        fill="#ffffff"
        stroke="#cbd5e1"
        strokeWidth={1}
        style={{ pointerEvents: 'none' }}
      />
      {/* 序号徽章 */}
      <rect
        x={x - 6} y={y - 6}
        width={16} height={16} rx={8}
        fill="#e0f2fe"
        stroke="#7dd3fc"
        strokeWidth={1}
      />
      <text
        x={x + 2} y={y + 6}
        textAnchor="middle" fontSize={9} fontWeight={700}
        fill="#0369a1"
        style={{ fontFamily: 'monospace' }}
      >
        {String(link.numericId + 1).padStart(2, '0')}
      </text>
      {/* 环节名称 */}
      <text
        x={x + 8} y={y + 22}
        fontSize={13} fontWeight={600}
        fill="#0f172a"
        style={{ fontFamily: 'system-ui' }}
      >
        {truncateText(link.name, 20)}
      </text>
      {/* prompt 摘要 */}
      {promptSnippet && (
        <text
          x={x + 8} y={y + 38}
          fontSize={10}
          fill="#64748b"
          style={{ fontFamily: 'system-ui', fontStyle: 'italic' }}
        >
          {truncateText(promptSnippet, 28)}
        </text>
      )}
      {/* executor */}
      <text
        x={x + NODE_WIDTH - 8} y={y + 16}
        textAnchor="end"
        fontSize={9}
        fill="#0369a1"
        style={{ fontFamily: 'monospace' }}
      >
        {executor}
      </text>
      {/* 门禁计数 + 重试次数 */}
      <text
        x={x + 8} y={y + NODE_HEIGHT - 10}
        fontSize={9}
        fill="#94a3b8"
        style={{ fontFamily: 'system-ui' }}
      >
        {gateCount > 0 ? `${gateCount} 门禁` : '无门禁'}
        {link.link.max_rework != null ? ` · 重试≤${link.link.max_rework}` : ' · 重试≤3'}
      </text>
      {/* 阶段名（靠右） */}
      <text
        x={x + NODE_WIDTH - 8} y={y + NODE_HEIGHT - 10}
        textAnchor="end"
        fontSize={9}
        fill="#94a3b8"
        style={{ fontFamily: 'system-ui' }}
      >
        {link.phaseName}
      </text>
    </g>
  );
}
