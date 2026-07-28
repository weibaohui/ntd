// ProcessVisualEditor.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：React Flow 主画布组件。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.7）：
// - 用 @xyflow/react 的 ReactFlow 组件渲染泳道编辑器。
// - 注册自定义节点类型：{ phase: PhaseNode, link: LinkNode }。
// - 注册自定义边类型：{ process: ProcessEdge }。
// - 通过 buildProcessGraph 从 ProcessDefinition 构建 nodes + edges。
// - onConnect → setLinkGoto → onDefinitionChange
// - onNodesDelete → removeLink → onDefinitionChange
//
// 数据流（M4 单向，可视化 → ProcessDefinition）：
//   definition → buildProcessGraph → nodes + edges → ReactFlow 渲染
//   用户操作（拖连线、删节点、改属性）→ onDefinitionChange(newDefinition)
//
// 非目标（留给后续里程碑）：
// - M5：双向联动 sync flag（可视化操作 → yaml.dump 刷新 Monaco）
// - M5：保存按钮
// ---------------------------------------------------------------------------

import { useMemo, useCallback, type CSSProperties, type JSX } from 'react';
import { Empty, Button } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  type Node,
  type Connection,
  type NodeMouseHandler,
  BackgroundVariant,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import type { ProcessDefinition } from '@/types/process';
import { buildProcessGraph } from './processGraphBuilder';
import {
  removeLink as _removeLink, // 暂未启用，M5 用于 onNodesDelete
  removePhase,
  addPhase,
  setLinkGoto,
  resetLinkGoto,
} from './processDefinitionUpdater';
// 避免未用导入告警：removeLink 暂保留
void _removeLink;
import { PhaseNode } from './nodes/PhaseNode';
import { LinkNode } from './nodes/LinkNode';
import { ProcessEdge } from './nodes/ProcessEdge';

// ── React Flow 类型注册 ────────────────────────────

// 自定义节点类型注册
// React Flow v12 要求 nodeTypes 是稳定引用（用 useMemo 或模块级常量）
const nodeTypes = {
  phase: PhaseNode,
  link: LinkNode,
};

// 自定义边类型注册
const edgeTypes = {
  process: ProcessEdge,
};

// ── Props 接口 ────────────────────────────────────

export interface ProcessVisualEditorProps {
  // 工艺定义（source of truth）
  definition: ProcessDefinition;
  // 定义变更回调（可视化操作触发）
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
  // 当前选中的节点 id（用于 React Flow 高亮）
  selectedNodeId: string | null;
  // 选中节点回调（点击节点触发）
  onSelectNode: (nodeId: string | null) => void;
  // 主题：'dark' | 'light'，影响 MiniMap 配色
  theme: 'dark' | 'light';
}

// ── 组件实现 ──────────────────────────────────────

export function ProcessVisualEditor({
  definition,
  onDefinitionChange,
  selectedNodeId,
  onSelectNode,
  theme,
}: ProcessVisualEditorProps): JSX.Element {
  // selectedNodeId 当前未用于 React Flow 高亮（PhaseNode/LinkNode 用自身 data.selected），
  // 保留 prop 以便 M5 实现选中态联动，此处占位避免未用告警。
  void selectedNodeId;
  // ── 回调集合 ────────────────────────────────────

  // 删除 phase 回调（弹 Modal.confirm，级联重置悬空 goto）
  // 注意：M4 这里只触发 onDefinitionChange，Modal.confirm 由 ProcessPropertyPanel 或上层处理
  // 实际上删除 phase 的入口是 PhaseNode 头部的删除按钮
  // 这里 onDeletePhase 直接调用 removePhase
  // Modal.confirm 的弹窗逻辑放在 ProcessEditor 层（M5 优化）
  const handleDeletePhase = useCallback(
    (phaseId: string) => {
      // 找到被删 phase 的 name，用于弹窗提示
      const phase = definition.phases?.find((p) => p.id === phaseId);
      const phaseName = phase?.name ?? phaseId;
      const linkCount = phase?.links?.length ?? 0;
      // 弹确认窗（用户必须知情同意删除）
      // 用 window.confirm 简化 M4，M5 换成 Ant Design Modal.confirm
      const message =
        linkCount > 0
          ? `删除阶段「${phaseName}」及其下 ${linkCount} 个环节？`
          : `删除阶段「${phaseName}」？`;
      if (window.confirm(message)) {
        const newDef = removePhase(definition, phaseId);
        onDefinitionChange(newDef);
      }
    },
    [definition, onDefinitionChange],
  );

  // ── M6 新增：空工艺 CTA 回调 ──────────────────────
  // 点击「新增阶段」按钮时生成第一个 phase，通过 onDefinitionChange 回写父组件。
  // 第一个 phase 的默认 id/name 用简单生成规则，避免空工艺用户还要想 id。
  // 第一个 phase 的 links 为空数组（空环节），用户后续在画布上添加环节。
  const handleAddPhase = useCallback(() => {
    // 简化默认：id 用 `phase-1`（首个 phase，无重名风险），name 用「阶段 1」
    const newDef = addPhase(definition, {
      id: 'phase-1',
      name: '阶段 1',
      links: [],
    });
    onDefinitionChange(newDef);
  }, [definition, onDefinitionChange]);

  // 选中 phase 回调
  const handleSelectPhase = useCallback(
    (phaseId: string) => {
      // phase 节点 id = phase-${phaseIndex}
      // 这里 phaseId 是 YAML 里的 phase.id，需要转成节点 id
      // buildProcessGraph 用 phaseIndex 生成节点 id
      // 为了简化，我们直接传 phase.id 给上层，上层在 selectedNodeId 里用 phase.id
      onSelectNode(phaseId);
    },
    [onSelectNode],
  );

  // 选中 link 回调
  const handleSelectLink = useCallback(
    (linkId: string) => {
      onSelectNode(linkId);
    },
    [onSelectNode],
  );

  // 删除边回调（重置对应 link 的 on_success / on_gate_fail）
  // 边 id 格式：edge-${sourceLinkId}-${handleType}-${targetLinkId}
  const handleDeleteEdge = useCallback(
    (edgeId: string) => {
      // 解析边 id：edge-link1-on_success-link2
      // 但 link id 可能含 '-'，用更稳健的方式：split('-') 后第 1 段是 source link id
      // 实际上 edge id 是 `edge-${sourceLinkId}-${handleType}-${targetLinkId}`
      // sourceLinkId 和 targetLinkId 可能含 '-'，handleType 是 'on_success' 或 'on_gate_fail'
      // 我们用 handleType 作为分割点
      const prefix = 'edge-';
      if (!edgeId.startsWith(prefix)) return;
      const rest = edgeId.slice(prefix.length);

      // 查找 handleType：on_success 或 on_gate_fail
      let handleType: 'on_success' | 'on_gate_fail' | null = null;
      let sourceLinkId = '';
      // targetLinkId 仅用于调试，此处下划线前缀避免未用告警
      let _targetLinkId = '';

      if (rest.includes('-on_success-')) {
        const parts = rest.split('-on_success-');
        sourceLinkId = parts[0];
        _targetLinkId = parts[1];
        handleType = 'on_success';
      } else if (rest.includes('-on_gate_fail-')) {
        const parts = rest.split('-on_gate_fail-');
        sourceLinkId = parts[0];
        _targetLinkId = parts[1];
        handleType = 'on_gate_fail';
      }
      void _targetLinkId;

      if (!handleType) return;

      // 找到 source link 所在的 phase
      let sourcePhaseId: string | null = null;
      for (const phase of definition.phases ?? []) {
        for (const link of phase.links ?? []) {
          if (link.id === sourceLinkId) {
            sourcePhaseId = phase.id;
            break;
          }
        }
        if (sourcePhaseId) break;
      }
      if (!sourcePhaseId) return;

      // 重置 on_success / on_gate_fail 为默认值
      const newDef = resetLinkGoto(
        definition,
        sourcePhaseId,
        sourceLinkId,
        handleType,
      );
      onDefinitionChange(newDef);
    },
    [definition, onDefinitionChange],
  );

  // ── 构建 nodes + edges ──────────────────────────

  // 用 useMemo 缓存 nodes + edges，避免每次渲染都重建
  // 依赖 [definition, handleDeletePhase, handleSelectPhase, handleSelectLink, handleDeleteEdge]
  const { nodes, edges } = useMemo(() => {
    return buildProcessGraph(definition, {
      onDeletePhase: handleDeletePhase,
      onSelectPhase: handleSelectPhase,
      onSelectLink: handleSelectLink,
      onDeleteEdge: handleDeleteEdge,
    });
  }, [
    definition,
    handleDeletePhase,
    handleSelectPhase,
    handleSelectLink,
    handleDeleteEdge,
  ]);

  // ── React Flow 事件处理 ─────────────────────────

  // 拖拽连线完成
  // connection.sourceHandle: 'on_success' | 'on_gate_fail'
  // connection.target: 目标节点 id（link-${phaseIndex}-${linkIndex}）
  const handleConnect = useCallback(
    (connection: Connection) => {
      // 从 source node id 找到 source link
      // source node id = link-${phaseIndex}-${linkIndex}
      // 我们需要 source link 的 YAML id（link.id）
      const sourceNodeId = connection.source;
      const handleType = connection.sourceHandle as
        | 'on_success'
        | 'on_gate_fail'
        | undefined;
      const targetNodeId = connection.target;

      if (!sourceNodeId || !handleType || !targetNodeId) return;

      // 从节点 id 解析 phaseIndex 和 linkIndex
      // link-0-0 → [0, 0]
      const sourceMatch = sourceNodeId.match(/^link-(\d+)-(\d+)$/);
      const targetMatch = targetNodeId.match(/^link-(\d+)-(\d+)$/);
      if (!sourceMatch || !targetMatch) return;

      const sourcePhaseIndex = parseInt(sourceMatch[1], 10);
      const sourceLinkIndex = parseInt(sourceMatch[2], 10);
      const targetPhaseIndex = parseInt(targetMatch[1], 10);
      const targetLinkIndex = parseInt(targetMatch[2], 10);
      // targetPhaseIndex/targetLinkIndex 供 M5 扩展使用，此处占位避免未用告警
      void targetPhaseIndex;
      void targetLinkIndex;

      // 从 definition 找到 source link 和 target link 的 YAML id
      const sourcePhase = definition.phases?.[sourcePhaseIndex];
      const targetPhase = definition.phases?.[targetPhaseIndex];
      const sourceLink = sourcePhase?.links?.[sourceLinkIndex];
      const targetLink = targetPhase?.links?.[targetLinkIndex];

      if (!sourceLink || !targetLink) return;

      // 设置 source link 的 on_success / on_gate_fail 为 goto:targetLinkId
      const newDef = setLinkGoto(
        definition,
        sourcePhase!.id,
        sourceLink.id,
        handleType,
        targetLink.id,
      );
      onDefinitionChange(newDef);
    },
    [definition, onDefinitionChange],
  );

  // 节点点击：选中节点（兜底路径）
  // PhaseNode 头部 onClick / LinkNode 容器 onClick 已触发 onSelectPhase/onSelectLink，
  // 但若用户点击命中在节点非头部区域（如 phase 容器整体），头部 onClick 未触发，
  // 这里兜底：从 node.id 反解 YAML id 调 onSelectNode，确保属性面板始终切换。
  // node.id 格式：phase-${i} | link-${i}-${j}，反解时直接传 node.id 给上层，
  // 上层 selectedNodeId 存 YAML id（phase.id / step.id），需做 id 映射。
  // 但 buildProcessGraph 用 phaseIndex 生成 node.id，YAML id 在 node.data 里，
  // 这里取 node.data.phase.id / node.data.step.id 兜底。
  const handleNodeClick: NodeMouseHandler = useCallback(
    (_, node) => {
      // 兜底：若 PhaseNode/LinkNode 内部 onClick 已触发，这里重复调无害（同 id 设置）
      // 从 node.data 取 YAML id，避免 node.id 与 YAML id 的索引偏移
      const data = node.data as Record<string, unknown>;
      if (node.type === 'phase' && data.phase) {
        const phase = data.phase as { id?: string };
        if (phase.id) onSelectNode(phase.id);
      } else if (node.type === 'link' && data.step) {
        const step = data.step as { id?: string };
        if (step.id) onSelectNode(step.id);
      }
    },
    [onSelectNode],
  );

  // 画布点击：取消选中
  const handlePaneClick = useCallback(() => {
    onSelectNode(null);
  }, [onSelectNode]);

  // ── 选中节点样式覆盖 ─────────────────────────────

  // React Flow 默认选中样式是 box-shadow，我们用 border 高亮
  // 这部分在 PhaseNode / LinkNode 的 selected prop 中处理

  // ── MiniMap 配色 ─────────────────────────────────

  // MiniMap 节点颜色：phase 灰色，link 白色
  const miniMapNodeColor = useCallback((node: Node) => {
    if (node.type === 'phase') return '#94a3b8';
    if (node.type === 'link') return '#fff';
    return '#94a3b8';
  }, []);

  // ── 渲染 ──────────────────────────────────────────

  // M6 空工艺分支：phases 为空数组或缺失时渲染 Empty + CTA 按钮，
  // 不渲染 React Flow（避免空画布只有 Controls/MiniMap 的奇怪态）。
  if (!definition.phases || definition.phases.length === 0) {
    return (
      <div style={emptyContainerStyle}>
        <Empty
          description="还没有任何阶段"
          image={Empty.PRESENTED_IMAGE_SIMPLE}
        >
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={handleAddPhase}
          >
            新增阶段
          </Button>
        </Empty>
      </div>
    );
  }

  // 非空工艺：渲染 React Flow（M4 已实现）
  return (
    <div style={containerStyle}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onConnect={handleConnect}
        onNodeClick={handleNodeClick}
        onPaneClick={handlePaneClick}
        // fitView 初始视口
        fitView
        // fitViewOptions 可控制 padding
        fitViewOptions={{ padding: 0.2 }}
        // 禁止节点拖出画布
        // 禁止边重复（同一 source-target 只能一条边）
        // defaultEdgeOptions 已在 buildProcessGraph 中设置
      >
        {/* 背景网格 */}
        <Background
          variant={BackgroundVariant.Dots}
          gap={20}
          size={1}
          color={theme === 'dark' ? '#334155' : '#cbd5e1'}
        />
        {/* 画布缩放控制 */}
        <Controls />
        {/* 小地图 */}
        <MiniMap
          nodeColor={miniMapNodeColor}
          nodeStrokeWidth={2}
          // MiniMap 背景色跟随主题
          style={{
            background: theme === 'dark' ? '#1e293b' : '#fff',
          }}
        />
      </ReactFlow>
    </div>
  );
}

// ── 样式 ──────────────────────────────────────────

// M6 空工艺容器：居中显示 Empty + CTA 按钮
const emptyContainerStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  justifyContent: 'center',
  alignItems: 'center',
  height: '100%',
  width: '100%',
};

const containerStyle: CSSProperties = {
  width: '100%',
  height: '100%',
  // 相对定位，确保 ReactFlow 内部绝对定位元素正确
  position: 'relative',
};
