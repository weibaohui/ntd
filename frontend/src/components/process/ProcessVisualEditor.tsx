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
  type Connection,
  type NodeMouseHandler,
  BackgroundVariant,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import type { ProcessDefinition } from '@/types/process';
import { buildProcessGraph } from './processGraphBuilder';
import {
  removeLink,
  removePhase,
  addPhase,
  addLink,
  setLinkGoto,
  resetLinkGoto,
} from './processDefinitionUpdater';
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
  // selectedNodeId 是选中态唯一数据源：既驱动右侧属性面板，也经 buildProcessGraph
  // 注入节点 data.selected 驱动画布高亮，保证「面板显示谁」与「图上谁高亮」一致。
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

  // ── M6 新增：新建阶段回调 ──────────────────────
  // 点击「+ 新增阶段」按钮时生成新 phase，通过 onDefinitionChange 回写父组件。
  // id 用动态 `phase-${n+1}`（n=现有 phase 数），避重复新建时 id 冲突失效。
  // name 用「阶段 ${n+1}」与 id 对齐，首个 phase 也走此规则（空工艺 n=0 → phase-1）。
  const handleAddPhase = useCallback(() => {
    const n = definition.phases?.length ?? 0;
    const newDef = addPhase(definition, {
      id: `phase-${n + 1}`,
      name: `阶段 ${n + 1}`,
      links: [],
    });
    onDefinitionChange(newDef);
  }, [definition, onDefinitionChange]);

  // ── M6 新增：新建环节回调 ──────────────────────
  // 在指定 phase 内追加 link，id 用 `link-${phaseId}-${n+1}`（n=该 phase 现有 link 数）。
  // name 用「环节 ${n+1}」与 id 对齐，避重复新建时 id 冲突。
  const handleAddLink = useCallback(
    (phaseId: string) => {
      const phase = definition.phases?.find((p) => p.id === phaseId);
      if (!phase) return;
      const n = phase.links?.length ?? 0;
      const newDef = addLink(definition, phaseId, {
        id: `link-${phaseId}-${n + 1}`,
        name: `环节 ${n + 1}`,
      });
      onDefinitionChange(newDef);
    },
    [definition, onDefinitionChange],
  );

  // 选中 phase 回调
  const handleSelectPhase = useCallback(
    (phaseId: string) => {
      // phase 节点 id = phase-${phaseIndex}
      // 这里 phaseId 是 YAML 里的 phase.id，需要转成节点 id
      // buildProcessGraph 用 phaseIndex 生成节点 id
      // 为了简化，我们直接传 phase.id 给上层，上层在 selectedNodeId 里用 phase.id
      // 再点一次已选中的 phase → 传 null 取消选中（与点击空白画布行为一致）
      onSelectNode(selectedNodeId === phaseId ? null : phaseId);
    },
    [onSelectNode, selectedNodeId],
  );

  // 选中 link 回调
  const handleSelectLink = useCallback(
    (linkId: string) => {
      // 再点一次已选中的 link → 传 null 取消选中
      onSelectNode(selectedNodeId === linkId ? null : linkId);
    },
    [onSelectNode, selectedNodeId],
  );

  // 删除 link 回调（LinkNode 删除按钮触发）：弹确认窗，调 removeLink 级联重置悬空 goto
  const handleDeleteLink = useCallback(
    (linkId: string) => {
      const link = definition.phases
        ?.flatMap((p) => p.links ?? [])
        .find((l) => l.id === linkId);
      const linkName = link?.name ?? linkId;
      if (window.confirm(`删除环节「${linkName}」？`)) {
        const newDef = removeLink(definition, linkId);
        onDefinitionChange(newDef);
      }
    },
    [definition, onDefinitionChange],
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
      onDeleteLink: handleDeleteLink,
      onDeleteEdge: handleDeleteEdge,
      onAddLink: handleAddLink,
      selectedNodeId,
    });
  }, [
    definition,
    handleDeletePhase,
    handleSelectPhase,
    handleSelectLink,
    handleDeleteLink,
    handleDeleteEdge,
    // selectedNodeId 变化需重建节点以刷新 data.selected 高亮
    selectedNodeId,
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
      // 兜底：内部 onClick 已 stopPropagation，只有点击节点空白区才走到这里。
      // 从 node.data 取 YAML id，避免 node.id 与 YAML id 的索引偏移。
      // 与内部选中回调同样做 toggle：再点一次已选中节点 → 取消选中。
      const data = node.data as Record<string, unknown>;
      // 注意：link 节点的 YAML 定义挂在 data.link（builder 注入），
      // 历史上误写成 data.step 导致该兜底分支从未命中，此处一并修正
      const yamlId =
        node.type === 'phase'
          ? (data.phase as { id?: string } | undefined)?.id
          : (data.link as { id?: string } | undefined)?.id;
      if (yamlId) onSelectNode(selectedNodeId === yamlId ? null : yamlId);
    },
    [onSelectNode, selectedNodeId],
  );

  // 画布点击：取消选中
  const handlePaneClick = useCallback(() => {
    onSelectNode(null);
  }, [onSelectNode]);

  // ── 选中节点样式覆盖 ─────────────────────────────

  // React Flow 默认选中样式是 box-shadow，我们用 border 高亮
  // 这部分在 PhaseNode / LinkNode 的 selected prop 中处理

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

  // 非空工艺：渲染 React Flow（M4 已实现）+ 顶部「+ 新增阶段」浮动按钮
  return (
    <div style={containerStyle}>
      {/* 顶部浮动「+ 新增阶段」按钮：絕对定位贴画布左上，z-index 高于 React Flow 节点 */}
      <Button
        type="primary"
        size="small"
        icon={<PlusOutlined />}
        onClick={handleAddPhase}
        style={addPhaseButtonStyle}
      >
        新增阶段
      </Button>
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
        {/* 小地图已按需求移除：泳道式布局横向狭长，MiniMap 辨识度低、占用角落空间 */}
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

// 顶部浮动「+ 新增阶段」按钮样式：絕对定位贴画布左上角
const addPhaseButtonStyle: CSSProperties = {
  position: 'absolute',
  top: 8,
  left: 8,
  zIndex: 10,
};
