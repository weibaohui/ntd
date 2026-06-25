import type { Node, Edge } from '@xyflow/react';
import type { Todo, Config, SlashCommandRule } from '@/types';
import { EXECUTORS } from '@/types';

export interface TodoNodeData extends Record<string, unknown> {
  title: string;
  status: string;
  executor: string;
  executorName: string;
  todoId: number;
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

export type RelationMapNode = Node<TodoNodeData | FeishuNodeData | SchedulerNodeData>;
export type RelationMapEdge = Edge<Record<string, unknown>>;

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

  // 如果没有纯 source，从 webhook/feishu/scheduler 类型开始
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
    // 防御环路：层级超过节点数则视为出现环路，停止继续提升
    if (layer > nodes.length) continue;
    const targets = edgeFrom.get(srcId) || [];
    for (const tId of targets) {
      const current = layers.get(tId) ?? -1;
      const newLayer = layer + 1;
      // 同样限制 newLayer 不得超过节点数，避免在有向环上无限自增
      if (newLayer > current && newLayer <= nodes.length) {
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

/** 收集被任意关系引用到的 Todo 节点和 id 集合 */
function collectReferencedTodoIds(
  todos: Todo[],
  config: Config | null,
): { referencedTodoIds: Set<number>; todoNodes: RelationMapNode[] } {
  const todoMap = new Map<number, Todo>();
  for (const t of todos) {
    todoMap.set(t.id, t);
  }

  const referencedTodoIds = new Set<number>();

  // 1. 飞书斜杠命令的目标 Todo
  for (const rule of config?.slash_command_rules || []) {
    if (rule.enabled) referencedTodoIds.add(rule.todo_id);
  }

  // 2. 飞书默认响应 Todo
  if (config?.default_response_todo_id) {
    referencedTodoIds.add(config.default_response_todo_id);
  }

  // 3. 启用了调度的 Todo
  for (const t of todos) {
    if (t.scheduler_enabled && t.scheduler_config) {
      referencedTodoIds.add(t.id);
    }
  }

  // 为所有被引用的 Todo 创建基础节点
  const todoNodes: RelationMapNode[] = [];
  for (const id of referencedTodoIds) {
    const t = todoMap.get(id);
    if (!t) continue;
    const executor = EXECUTORS.find(e => e.value === (t.executor || 'claudecode'));
    todoNodes.push({
      id: `todo-${id}`,
      type: 'todo',
      position: { x: 0, y: 0 },
      data: {
        title: t.title,
        status: t.status,
        executor: t.executor || 'claudecode',
        executorName: executor?.label || t.executor || 'Claude',
        todoId: id,
      },
    });
  }

  return { referencedTodoIds, todoNodes };
}

/**
 * 主构建函数
 *
 * todo hook 已整块移除（plan `purring-forging-petal`），不再构建 Todo→Todo 的
 * hook 边，也不再需要 `showHooks` 参数；上游调用方需要相应地收敛开关 UI。
 */
export function buildRelationMap(
  todos: Todo[],
  config: Config | null,
  showFeishu: boolean,
  showScheduler: boolean,
): { nodes: RelationMapNode[]; edges: RelationMapEdge[] } {
  const allNodes: RelationMapNode[] = [];
  const allEdges: RelationMapEdge[] = [];

  // 收集被任意关系引用到的 Todo 节点和 id 集合
  const { referencedTodoIds, todoNodes } = collectReferencedTodoIds(todos, config);
  allNodes.push(...todoNodes);

  // 飞书和调度器关系
  if (showFeishu || showScheduler) {
    const { nodes, edges } = buildTriggerSourceRelations(todos, config, referencedTodoIds);
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
