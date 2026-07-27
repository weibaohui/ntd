// ProcessPropertyPanel.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：右侧属性面板路由组件。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.8）：
// - 根据 selectedNodeId 切换到对应的属性表单。
// - selectedNodeId 为 phase 的 YAML id → PhasePropertyForm
// - selectedNodeId 为 link 的 YAML id → LinkPropertyForm
// - selectedNodeId 为 null → GlobalPropertyForm（全局面板）
//
// 数据流（M4 单向）：
//   definition + selectedNodeId → 匹配表单 → 渲染
//   表单字段修改 → onDefinitionChange(newDefinition)
//
// 注意：selectedNodeId 存的是 YAML id（phase.id / link.id），
// 不是 React Flow 节点 id（phase-${i} / link-${i}-${j}）。
// 这是为了让属性面板在节点增删（索引变化）时仍能正确定位。
// ---------------------------------------------------------------------------

import { type CSSProperties, type JSX } from 'react';
import { Typography, Empty } from 'antd';
import type { ProcessDefinition } from '@/types/process';
import { LinkPropertyForm } from './propertyForms/LinkPropertyForm';
import { PhasePropertyForm } from './propertyForms/PhasePropertyForm';
import { GlobalPropertyForm } from './propertyForms/GlobalPropertyForm';

const { Text } = Typography;

export interface ProcessPropertyPanelProps {
  // 工艺定义（source of truth）
  definition: ProcessDefinition;
  // 当前选中的节点 id（YAML id，不是 React Flow 节点 id）
  // null 表示未选中，显示全局面板
  selectedNodeId: string | null;
  // 定义变更回调
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
}

export function ProcessPropertyPanel({
  definition,
  selectedNodeId,
  onDefinitionChange,
}: ProcessPropertyPanelProps): JSX.Element {
  // ── 解析 selectedNodeId ─────────────────────────
  // null → 全局面板
  if (selectedNodeId === null) {
    return (
      <div style={containerStyle}>
        <GlobalPropertyForm
          definition={definition}
          onDefinitionChange={onDefinitionChange}
        />
      </div>
    );
  }

  // ── 查找 selectedNodeId 对应的 phase / link ─────
  // 先查 phase（selectedNodeId === phase.id）
  const phase = definition.phases?.find((p) => p.id === selectedNodeId);

  // 再查 link（selectedNodeId === link.id）
  let linkPhaseId: string | null = null;
  let linkFound = false;
  for (const p of definition.phases ?? []) {
    for (const l of p.links ?? []) {
      if (l.id === selectedNodeId) {
        linkPhaseId = p.id;
        linkFound = true;
        break;
      }
    }
    if (linkFound) break;
  }

  // ── 渲染对应表单 ─────────────────────────────────

  // 选中 phase
  if (phase) {
    return (
      <div style={containerStyle}>
        <PhasePropertyForm
          definition={definition}
          phaseId={phase.id}
          onDefinitionChange={onDefinitionChange}
        />
      </div>
    );
  }

  // 选中 link
  if (linkFound && linkPhaseId) {
    return (
      <div style={containerStyle}>
        <LinkPropertyForm
          definition={definition}
          phaseId={linkPhaseId}
          linkId={selectedNodeId}
          onDefinitionChange={onDefinitionChange}
        />
      </div>
    );
  }

  // selectedNodeId 既不匹配 phase 也不匹配 link（悬空引用）
  // 回退到全局面板
  return (
    <div style={containerStyle}>
      <Empty
        description={
          <Text type="secondary">
            选中的节点「{selectedNodeId}」不存在，已回退到全局面板
          </Text>
        }
      />
      <GlobalPropertyForm
        definition={definition}
        onDefinitionChange={onDefinitionChange}
      />
    </div>
  );
}

// ── 样式 ──────────────────────────────────────────

const containerStyle: CSSProperties = {
  height: '100%',
  overflow: 'auto',
  // 浅灰背景，与可视化区区分
  background: '#f8fafc',
  // 左边框分隔
  borderLeft: '1px solid #e2e8f0',
};
