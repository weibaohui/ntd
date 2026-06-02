import type { Node, Edge } from '@xyflow/react';
import type { Todo, Config, SlashCommandRule } from '../../types';
import type { Webhook } from '../../utils/database/webhooks';
import { EXECUTORS } from '../../types';

export interface TodoNodeData extends Record<string, unknown> {
  title: string;
  status: string;
  executor: string;
  executorName: string;
  hasHooks: boolean;
  todoId: number;
}

export interface WebhookNodeData extends Record<string, unknown> {
  name: string;
  enabled: boolean;
  webhookId: number;
  defaultTodoId: number | null;
}

export interface FeishuNodeData extends Record<string, unknown> {
  label: string;
  slashCommand?: string;
  todoId: number;
  type: 'slash_command' | 'default_response';
}

export interface SchedulerNodeData extends Record<string, unknown> {
  label: string;
  cron: string;
  todoId: number;
}

export interface HookEdgeData extends Record<string, unknown> {
  trigger: string;
  hookId: number;
  enabled: boolean;
}

export interface WebhookEdgeData extends Record<string, unknown> {
  webhookId: number;
  webhookName: string;
}

export type RelationMapNode = Node<TodoNodeData | WebhookNodeData | FeishuNodeData | SchedulerNodeData>;
export type RelationMapEdge = Edge<HookEdgeData | WebhookEdgeData | Record<string, unknown>>;

/** 构建 Todo → Hook → Todo 的节点和边 */
function buildHookRelations(
  todos: Todo[],
): { nodes: RelationMapNode[]; edges: RelationMapEdge[] } {
  const nodes: RelationMapNode[] = [];
  const edges: RelationMapEdge[] = [];
  const todoMap = new Map<number, Todo>();

  for (const t of todos) {
    todoMap.set(t.id, t);
  }
  const hookTodoIds = new Set<number>();
  for (const t of todos) {
    if (t.hooks && t.hooks.length > 0) {
      hookTodoIds.add(t.id);
      for (const h of t.hooks) {
        if (h.enabled) hookTodoIds.add(h.target_todo_id);
      }
    }
  }

  // 创建 Todo 节点
  let todoIndex = 0;
  for (const id of hookTodoIds) {
    const t = todoMap.get(id);
    if (!t) continue;
    const executor = EXECUTORS.find(e => e.value === (t.executor || 'claudecode'));
    nodes.push({
      id: `todo-${id}`,
      type: 'todo',
      position: { x: 0, y: 0 }, // layout 会重新计算
      data: {
        title: t.title,
        status: t.status,
        executor: t.executor || 'claudecode',
        executorName: executor?.label || t.executor || 'Claude',
        hasHooks: !!(t.hooks && t.hooks.length > 0),
        todoId: id,
      },
    });
    todoIndex++;
  }

  // 创建 Hook 边
  for (const t of todos) {
    if (!t.hooks) continue;
    for (const h of t.hooks) {
      if (!h.enabled) continue;
      if (!hookTodoIds.has(h.target_todo_id)) continue;
      edges.push({
        id: `hook-${h.id}`,
        source: `todo-${t.id}`,
        target: `todo-${h.target_todo_id}`,
        type: 'hook',
        data: {
          trigger: h.trigger,
          hookId: h.id,
          enabled: h.enabled,
        },
        animated: false,
      });
    }
  }

  return { nodes, edges };
}

/** 构建 Webhook → Todo 的节点和边 */
function buildWebhookRelations(
  webhooks: Webhook[],
  existingTodoIds: Set<number>,
): { nodes: RelationMapNode[]; edges: RelationMapEdge[] } {
  const nodes: RelationMapNode[] = [];
  const edges: RelationMapEdge[] = [];

  for (const wh of webhooks) {
    if (wh.default_todo_id == null) continue;
    nodes.push({
      id: `webhook-${wh.id}`,
      type: 'webhook',
      position: { x: 0, y: 0 },
      data: {
        name: wh.name,
        enabled: wh.enabled,
        webhookId: wh.id,
        defaultTodoId: wh.default_todo_id,
      },
    });

    const targetId = `todo-${wh.default_todo_id}`;
    // 只关联已存在的 todo 节点
    if (existingTodoIds.has(wh.default_todo_id)) {
      edges.push({
        id: `webhook-edge-${wh.id}`,
        source: `webhook-${wh.id}`,
        target: targetId,
        type: 'webhook',
        data: {
          webhookId: wh.id,
          webhookName: wh.name,
        },
        animated: false,
      });
    }
  }

  return { nodes, edges };
}

/** 构建飞书消息 → Todo 和 调度器 → Todo 的节点和边 */
function buildTriggerSourceRelations(
  todos: Todo[],
  config: Config | null,
  existingTodoIds: Set<number>,
): { nodes: RelationMapNode[]; edges: RelationMapEdge[] } {
  const nodes: RelationMapNode[] = [];
  const edges: RelationMapEdge[] = [];

  // 斜杠命令规则
  const slashRules: SlashCommandRule[] = config?.slash_command_rules || [];
  for (const rule of slashRules) {
    if (!rule.enabled) continue;
    if (!existingTodoIds.has(rule.todo_id)) continue;
    const node: RelationMapNode = {
      id: `feishu-slash-${rule.todo_id}`,
      type: 'feishu',
      position: { x: 0, y: 0 },
      data: {
        label: '飞书命令',
        slashCommand: rule.slash_command,
        todoId: rule.todo_id,
        type: 'slash_command',
      },
    };
    nodes.push(node);
    edges.push({
      id: `feishu-slash-edge-${rule.todo_id}`,
      source: `feishu-slash-${rule.todo_id}`,
      target: `todo-${rule.todo_id}`,
      type: 'feishu',
      data: { triggerType: 'slash_command' },
      animated: false,
    });
  }

  // 默认响应 Todo
  const defaultResponseTodoId = config?.default_response_todo_id;
  if (defaultResponseTodoId && existingTodoIds.has(defaultResponseTodoId)) {
    nodes.push({
      id: 'feishu-default',
      type: 'feishu',
      position: { x: 0, y: 0 },
      data: {
        label: '飞书消息',
        todoId: defaultResponseTodoId,
        type: 'default_response',
      },
    });
    edges.push({
      id: 'feishu-default-edge',
      source: 'feishu-default',
      target: `todo-${defaultResponseTodoId}`,
      type: 'feishu',
      data: { triggerType: 'default_response' },
      animated: false,
    });
  }

  // 调度器节点
  for (const t of todos) {
    if (!t.scheduler_enabled || !t.scheduler_config) continue;
    if (!existingTodoIds.has(t.id)) continue;
    nodes.push({
      id: `scheduler-${t.id}`,
      type: 'scheduler',
      position: { x: 0, y: 0 },
      data: {
        label: '定时调度',
        cron: t.scheduler_config,
        todoId: t.id,
      },
    });
    edges.push({
      id: `scheduler-edge-${t.id}`,
      source: `scheduler-${t.id}`,
      target: `todo-${t.id}`,
      type: 'scheduler',
      data: {},
      animated: false,
    });
  }

  return { nodes, edges };
}

/** 简单的分层布局算法（从左到右） */
function applyLayout(nodes: RelationMapNode[], edges: RelationMapEdge[]): RelationMapNode[] {
  // 分层：source 节点在左，纯目标在右
  const sourceIds = new Set<string>();
  const targetIds = new Set<string>();

  for (const e of edges) {
    sourceIds.add(e.source);
    targetIds.add(e.target);
  }

  // 拓扑排序分层
  const layers = new Map<string, number>();

  // BFS 分层
  const queue: Array<{ id: string; layer: number }> = [];

  // source 节点（没有入边的）放在第 0 层
  for (const n of nodes) {
    if (!targetIds.has(n.id)) {
      layers.set(n.id, 0);
      queue.push({ id: n.id, layer: 0 });
    }
  }

  // 如果没有纯 source，从 hook/webhook/feishu/scheduler 类型开始
  if (queue.length === 0) {
    for (const n of nodes) {
      if (n.type !== 'todo') {
        layers.set(n.id, 0);
        queue.push({ id: n.id, layer: 0 });
      }
    }
  }

  // 如果还是没有，取第一个
  if (queue.length === 0 && nodes.length > 0) {
    layers.set(nodes[0].id, 0);
    queue.push({ id: nodes[0].id, layer: 0 });
  }

  const edgeFrom = new Map<string, string[]>();
  for (const e of edges) {
    const list = edgeFrom.get(e.source) || [];
    list.push(e.target);
    edgeFrom.set(e.source, list);
  }

  while (queue.length > 0) {
    const { id: srcId, layer } = queue.shift()!;
    const targets = edgeFrom.get(srcId) || [];
    for (const tId of targets) {
      const current = layers.get(tId) ?? -1;
      const newLayer = layer + 1;
      if (newLayer > current) {
        layers.set(tId, newLayer);
        queue.push({ id: tId, layer: newLayer });
      }
    }
  }

  // 未被分配层次的节点
  for (const n of nodes) {
    if (!layers.has(n.id)) {
      layers.set(n.id, 1);
    }
  }

  // 按层分组
  const layerGroups = new Map<number, RelationMapNode[]>();
  for (const n of nodes) {
    const l = layers.get(n.id) || 0;
    const group = layerGroups.get(l) || [];
    group.push(n);
    layerGroups.set(l, group);
  }

  const H_GAP = 280;
  const V_GAP = 100;
  const result: RelationMapNode[] = [];

  const sortedLayers = Array.from(layerGroups.entries()).sort((a, b) => a[0] - b[0]);
  for (const [, group] of sortedLayers) {
    // 按类型排序：source 类型在上，todo 在下
    const sorted = [...group].sort((a, b) => {
      const typeOrder: Record<string, number> = { webhook: 0, feishu: 1, scheduler: 2, todo: 3 };
      return (typeOrder[a.type || ''] ?? 3) - (typeOrder[b.type || ''] ?? 3);
    });

    const totalHeight = sorted.length * V_GAP;
    const startY = -totalHeight / 2;

    for (let i = 0; i < sorted.length; i++) {
      const layer = layers.get(sorted[i].id) || 0;
      result.push({
        ...sorted[i],
        position: {
          x: layer * H_GAP,
          y: startY + i * V_GAP,
        },
      });
    }
  }

  return result;
}

/** 主构建函数 */
export function buildRelationMap(
  todos: Todo[],
  webhooks: Webhook[],
  config: Config | null,
  showHooks: boolean,
  showWebhooks: boolean,
  showFeishu: boolean,
  showScheduler: boolean,
): { nodes: RelationMapNode[]; edges: RelationMapEdge[] } {
  const allNodes: RelationMapNode[] = [];
  const allEdges: RelationMapEdge[] = [];

  // Hook 关系
  if (showHooks) {
    const { nodes, edges } = buildHookRelations(todos);
    allNodes.push(...nodes);
    allEdges.push(...edges);
  }

  const existingTodoIds = new Set(allNodes.map(n => n.data.todoId as number).filter(Boolean));

  // Webhook 关系
  if (showWebhooks) {
    const { nodes, edges } = buildWebhookRelations(webhooks, existingTodoIds);
    allNodes.push(...nodes);
    allEdges.push(...edges);
  }

  // 飞书和调度器关系
  if (showFeishu || showScheduler) {
    const { nodes, edges } = buildTriggerSourceRelations(todos, config, existingTodoIds);
    for (const n of nodes) {
      const isFeishu = n.type === 'feishu';
      const isScheduler = n.type === 'scheduler';
      if ((isFeishu && showFeishu) || (isScheduler && showScheduler)) {
        allNodes.push(n);
      }
    }
    for (const e of edges) {
      const isFeishuEdge = e.type === 'feishu';
      const isSchedulerEdge = e.type === 'scheduler';
      if ((isFeishuEdge && showFeishu) || (isSchedulerEdge && showScheduler)) {
        allEdges.push(e);
      }
    }
  }

  // 去重节点（同一个 todo 可能被多种方式关联）
  const uniqueNodes = new Map<string, RelationMapNode>();
  for (const n of allNodes) {
    uniqueNodes.set(n.id, n);
  }

  // 去重边
  const uniqueEdges = new Map<string, RelationMapEdge>();
  for (const e of allEdges) {
    uniqueEdges.set(e.id, e);
  }

  const nodes = applyLayout(Array.from(uniqueNodes.values()), Array.from(uniqueEdges.values()));

  return { nodes, edges: Array.from(uniqueEdges.values()) };
}
