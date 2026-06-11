import { useState, useEffect, useMemo } from 'react';
import {
  ReactFlow,
  Background,
  MiniMap,
  useNodesState,
  useEdgesState,
  type NodeTypes,
  type EdgeTypes,
  Panel,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { Switch, Empty, Spin, Button } from 'antd';
import {
  ApiOutlined,
  MessageOutlined,
  ScheduleOutlined,
  LinkOutlined,
  LeftOutlined,
} from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import { useTheme } from '@/hooks/useTheme';
import * as db from '@/utils/database';
import type { Webhook } from '@/utils/database/webhooks';
import type { Config } from '@/types';
import { TodoNode, WebhookNode, FeishuNode, SchedulerNode } from './Nodes';
import { HookEdge, WebhookEdge, FeishuEdge, SchedulerEdge } from './Edges';
import { buildRelationMap } from './GraphBuilder';
import './relation-map.css';

const nodeTypes: NodeTypes = {
  todo: TodoNode,
  webhook: WebhookNode,
  feishu: FeishuNode,
  scheduler: SchedulerNode,
};

const edgeTypes: EdgeTypes = {
  hook: HookEdge,
  webhook: WebhookEdge,
  feishu: FeishuEdge,
  scheduler: SchedulerEdge,
};

interface RelationMapProps {
  onBack?: () => void;
}

export function RelationMap({ onBack }: RelationMapProps) {
  const { state } = useApp();
  const { themeMode } = useTheme();
  const [webhooks, setWebhooks] = useState<Webhook[]>([]);
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);

  // 过滤器
  const [showHooks, setShowHooks] = useState(true);
  const [showWebhooks, setShowWebhooks] = useState(true);
  const [showFeishu, setShowFeishu] = useState(true);
  const [showScheduler, setShowScheduler] = useState(true);

  // 加载额外数据
  useEffect(() => {
    Promise.all([
      db.getWebhooks().catch(() => []),
      db.getConfig().catch(() => null),
    ]).then(([wh, cfg]) => {
      setWebhooks(wh as Webhook[]);
      setConfig(cfg as Config | null);
      setLoading(false);
    });
  }, []);

  // 构建图数据
  const { nodes: builtNodes, edges: builtEdges } = useMemo(
    () => buildRelationMap(state.todos, webhooks, config, showHooks, showWebhooks, showFeishu, showScheduler),
    [state.todos, webhooks, config, showHooks, showWebhooks, showFeishu, showScheduler],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(builtNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(builtEdges);

  // 当构建数据变化时同步
  useEffect(() => {
    setNodes(builtNodes);
    setEdges(builtEdges);
  }, [builtNodes, builtEdges, setNodes, setEdges]);

  // 实时状态更新：当有 todo 状态变化时，更新对应节点的数据
  useEffect(() => {
    setNodes(nds =>
      nds.map(n => {
        if (n.type !== 'todo') return n;
        const todoId = n.data?.todoId as number;
        if (!todoId) return n;
        const todo = state.todos.find(t => t.id === todoId);
        if (!todo) return n;
        if (n.data?.status === todo.status) return n;
        return {
          ...n,
          data: {
            ...n.data,
            status: todo.status,
          },
        };
      }),
    );
  }, [state.todos, setNodes]);

  const isDark = themeMode === 'dark';

  if (loading) {
    return (
      <div className="relation-map-loading">
        <Spin size="large" description="加载关联图数据..." />
      </div>
    );
  }

  return (
    <div className={`relation-map-container ${isDark ? 'dark' : 'light'}`}>
      {onBack && (
        <Button
          type="text"
          size="small"
          icon={<LeftOutlined />}
          onClick={onBack}
          className="relation-map-back-btn"
          aria-label="返回"
        />
      )}
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        fitViewOptions={{ padding: 0.6 }}
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        className="relation-map-canvas"
      >
        <Background
          color={isDark ? '#333' : '#ddd'}
          gap={20}
          size={1}
        />
        <MiniMap
          nodeColor={(node) => {
            if (node.type === 'webhook') return '#722ed1';
            if (node.type === 'feishu') return '#1890ff';
            if (node.type === 'scheduler') return '#fa8c16';
            // todo 节点按状态着色
            const status = node.data?.status as string;
            switch (status) {
              case 'running': return '#1677ff';
              case 'completed': return '#52c41a';
              case 'failed': return '#ff4d4f';
              default: return '#8c8c8c';
            }
          }}
          maskColor={isDark ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)'}
        />

        {/* 右侧过滤面板 */}
        <Panel position="top-right" className="relation-map-filters">
          <div className="filter-group">
            <div className="filter-item">
              <LinkOutlined style={{ color: '#52c41a', marginRight: 4 }} />
              <span className="filter-label">Hook</span>
              <Switch size="small" checked={showHooks} onChange={setShowHooks} />
            </div>
            <div className="filter-item">
              <ApiOutlined style={{ color: '#722ed1', marginRight: 4 }} />
              <span className="filter-label">Webhook</span>
              <Switch size="small" checked={showWebhooks} onChange={setShowWebhooks} />
            </div>
            <div className="filter-item">
              <MessageOutlined style={{ color: '#1890ff', marginRight: 4 }} />
              <span className="filter-label">飞书</span>
              <Switch size="small" checked={showFeishu} onChange={setShowFeishu} />
            </div>
            <div className="filter-item">
              <ScheduleOutlined style={{ color: '#fa8c16', marginRight: 4 }} />
              <span className="filter-label">调度</span>
              <Switch size="small" checked={showScheduler} onChange={setShowScheduler} />
            </div>
          </div>
        </Panel>

        {/* 底部图例 */}
        <Panel position="bottom-left" className="relation-map-legend">
          <div className="legend-item">
            <span className="legend-line solid" style={{ background: '#52c41a' }} />
            <span>Hook（实线）</span>
          </div>
          <div className="legend-item">
            <span className="legend-line dashed" style={{ background: '#722ed1' }} />
            <span>Webhook（虚线）</span>
          </div>
          <div className="legend-item">
            <span className="legend-line dotted" style={{ background: '#1890ff' }} />
            <span>飞书消息（点线）</span>
          </div>
          <div className="legend-item">
            <span className="legend-line dashed-long" style={{ background: '#fa8c16' }} />
            <span>定时调度（长虚线）</span>
          </div>
        </Panel>

        {/* 空状态提示 - 画板内居中 */}
        {builtNodes.length === 0 && (
          <Panel position="top-center" className="relation-map-empty-panel">
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={
                <div>
                  <p style={{ color: 'var(--color-text-secondary)', marginBottom: 8 }}>暂无关联关系</p>
                  <p style={{ color: 'var(--color-text-tertiary)', fontSize: 13 }}>
                    为 Todo 添加 Hook 或配置 Webhook 后，关联关系将在此显示
                  </p>
                </div>
              }
            />
          </Panel>
        )}
      </ReactFlow>
    </div>
  );
}
