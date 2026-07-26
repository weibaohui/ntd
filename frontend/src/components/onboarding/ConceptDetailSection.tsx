// 概念详细说明区：每个概念一段，左右分栏。
// 左栏：定义 + 字段表（Descriptions）。
// 右栏：实际数据快照（从后端拉一条真实记录展示），空态时展示 YAML 示例 + 跳转入口。
// 底部：「去 XX 页」跳转按钮。

import { useEffect, useState, useCallback } from 'react';
import { Typography, Descriptions, Button, Spin, Empty, Tag, Table } from 'antd';
import { ArrowRightOutlined, CheckCircleOutlined, InboxOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import * as db from '@/utils/database';
import * as dbLoops from '@/utils/database/loops';
import { useViewState } from '@/hooks/useViewState';
import {
  CONCEPTS,
  EXECUTOR_VS_EXPERT_VS_MODEL,
  TRIGGER_TYPES,
  GATE_TYPES,
  type ConceptNode,
} from '@/components/onboarding/concepts';

const { Text, Title, Paragraph } = Typography;

interface ConceptDetailSectionProps {
  concept: ConceptNode;
  /** 工作空间 id，用于拉数据快照。 */
  workspaceId: number | null;
}

/** 数据快照类型：一条真实记录的原始数据（具体类型由后端定，这里用 object 兜底）。 */
type SnapshotData = object | null;

/**
 * 按 concept.id 拉一条真实记录。
 *
 * 各概念对应的 API：
 *   process → bundledApi.getProcesses() 取首条
 *   loop    → dbLoops.listLoops(wsId) 取首条
 *   todo    → db.getAllTodos(wsId) 取首条
 *   task    → bundledApi.listTasks(wsId) 取首条
 *   executor → db.getExecutors() 取首条
 *   expert  → db.getAllExperts() 取首条
 *
 * 失败或空 → null，由 EmptyState 兜底。
 */
async function fetchSnapshot(
  conceptId: ConceptNode['id'],
  wsId: number,
): Promise<SnapshotData> {
  switch (conceptId) {
    case 'process': {
      const list = await bundledApi.getProcesses();
      return list.length > 0 ? list[0] : null;
    }
    case 'loop': {
      const list = await dbLoops.listLoops(wsId);
      return list.length > 0 ? list[0] : null;
    }
    case 'todo': {
      const list = await db.getAllTodos(wsId);
      return list.length > 0 ? list[0] : null;
    }
    case 'task': {
      const list = await bundledApi.listTasks(wsId);
      return list.length > 0 ? list[0] : null;
    }
    case 'executor': {
      const list = await db.getExecutors();
      return list.length > 0 ? list[0] : null;
    }
    case 'expert': {
      const list = await db.getAllExperts();
      return list.length > 0 ? list[0] : null;
    }
    default:
      return null;
  }
}

/** 把快照对象渲染为关键字段表（过滤敏感字段）。 */
function renderSnapshotTable(data: SnapshotData): React.ReactNode {
  if (!data || typeof data !== 'object') return null;
  // 过滤敏感字段：不展示 app_secret/bot_open_id/app_id 等。
  const sensitiveKeys = ['app_secret', 'bot_open_id', 'app_id', 'owner_open_id', 'session_dir'];
  // Object.entries 对 object 类型安全，窄化为 [string, unknown][]。
  const entries = Object.entries(data as Record<string, unknown>).filter(([k]) => !sensitiveKeys.includes(k));
  // 只展示前 8 个字段，避免表格过长。
  const visible = entries.slice(0, 8);
  return (
    <Table
      size="small"
      pagination={false}
      dataSource={visible.map(([k, v]) => ({ key: k, value: String(v ?? '') }))}
      rowKey={(r) => r.key}
      columns={[
        { title: '字段', dataIndex: 'key', key: 'key', width: 120 },
        { title: '值', dataIndex: 'value', key: 'value', ellipsis: true },
      ]}
    />
  );
}

/** 数据快照展示：真实记录的字段表。 */
function SnapshotView({ data }: { data: SnapshotData }) {
  return (
    <div data-testid="onboarding-detail-snapshot">
      <Text type="secondary" style={{ fontSize: 12, marginBottom: 8, display: 'block' }}>
        <CheckCircleOutlined style={{ marginRight: 4, color: '#52c41a' }} />
        来自你系统的真实数据
      </Text>
      {renderSnapshotTable(data)}
    </div>
  );
}

/** 空态：无数据时展示 YAML 示例 + 跳转入口。 */
function EmptyState({
  concept,
  onGoto,
}: {
  concept: ConceptNode;
  onGoto: () => void;
}) {
  return (
    <div data-testid="onboarding-detail-empty">
      <Empty
        image={<InboxOutlined style={{ fontSize: 32, color: '#bfbfbf' }} />}
        description={`暂无${concept.label}数据`}
        style={{ marginBottom: 16 }}
      />
      {/* YAML 示例：让用户知道该概念的数据长什么样 */}
      <Text type="secondary" style={{ fontSize: 12, display: 'block', marginBottom: 8 }}>
        示例数据格式：
      </Text>
      <pre
        style={{
          background: 'var(--color-fill-quaternary, #f5f5f5)',
          padding: 12,
          borderRadius: 6,
          fontSize: 12,
          overflow: 'auto',
          maxHeight: 200,
          margin: 0,
        }}
      >
        {concept.yamlExample}
      </pre>
      <Button type="link" icon={<ArrowRightOutlined />} onClick={onGoto} style={{ marginTop: 8, padding: 0 }}>
        去{concept.label}页创建
      </Button>
    </div>
  );
}

/** 执行器 vs 专家 vs 模型对比表（仅 executor 概念展示）。 */
function ExecutorVsExpertTable() {
  return (
    <div style={{ marginTop: 16 }}>
      <Text strong style={{ display: 'block', marginBottom: 8 }}>
        执行器 vs 专家 vs 模型（三者正交）
      </Text>
      <Table
        size="small"
        pagination={false}
        dataSource={EXECUTOR_VS_EXPERT_VS_MODEL.map((r, i) => ({ ...r, key: i }))}
        columns={[
          { title: '概念', dataIndex: 'concept', key: 'concept', width: 80 },
          { title: '本质', dataIndex: 'essence', key: 'essence' },
          { title: '例子', dataIndex: 'example', key: 'example' },
        ]}
      />
      <Paragraph type="secondary" style={{ fontSize: 12, marginTop: 8, marginBottom: 0 }}>
        优先级链：事项级 model &gt; 执行器 default_model &gt; 执行器配置文件。
        同一执行器可加载不同专家（如 claudecode + product-manager）。
      </Paragraph>
    </div>
  );
}

/** 触发器类型表（仅 loop 概念展示）。 */
function TriggerTypesTable() {
  return (
    <div style={{ marginTop: 16 }}>
      <Text strong style={{ display: 'block', marginBottom: 8 }}>
        触发器 8 种类型
      </Text>
      <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
        {TRIGGER_TYPES.map((t) => (
          <Tag key={t.type} style={{ marginBottom: 4 }}>
            {t.type} · {t.label}
          </Tag>
        ))}
      </div>
    </div>
  );
}

/** 门禁类型表（仅 loop/todo 概念展示）。 */
function GateTypesTable() {
  return (
    <div style={{ marginTop: 16 }}>
      <Text strong style={{ display: 'block', marginBottom: 8 }}>
        门禁 4 种类型（验收机制）
      </Text>
      <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
        {GATE_TYPES.map((g) => (
          <Tag key={g.type} color="purple" style={{ marginBottom: 4 }}>
            {g.type} · {g.label}
          </Tag>
        ))}
      </div>
    </div>
  );
}

/**
 * 单个概念的详细说明区。
 *
 * 整体处理思路：
 * 1. 左右分栏 grid（1fr 1fr），左栏字段表，右栏数据快照。
 * 2. 数据快照按 concept.id 拉对应 API 首条记录。
 * 3. 空态时展示 YAML 示例 + 跳转入口。
 * 4. 底部「去 XX 页」按钮调 pushUrl 路由跳转。
 * 5. 特定概念附加对比表：executor 展示三者对比，loop 展示触发器+门禁类型。
 */
export function ConceptDetailSection({ concept, workspaceId }: ConceptDetailSectionProps) {
  // undefined = 未拉取，null = 拉取完但无数据。
  // useState 兜底用 null，初始显式传 null 避免类型不宽。
  const [snapshot, setSnapshot] = useState<SnapshotData | undefined>(undefined);
  const [loading, setLoading] = useState(false);
  const { pushUrl } = useViewState();

  // null workspace 回退到 1（与 TasksPage 一致）。
  const wsId = workspaceId ?? 1;

  // 拉数据快照：concept.id 或 wsId 变化时重拉。
  // undefined = 未拉取，null = 拉取完但无数据。
  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await fetchSnapshot(concept.id, wsId);
      setSnapshot(data);
    } catch {
      setSnapshot(null);
    } finally {
      setLoading(false);
    }
  }, [concept.id, wsId]);

  useEffect(() => {
    load();
  }, [load]);

  const handleGoto = () => {
    pushUrl(concept.navTarget, {});
  };

  return (
    <section
      id={`concept-${concept.id}`}
      data-testid={`onboarding-detail-${concept.id}`}
      style={{
        // 上下间距：与相邻 section 分隔。
        padding: '24px 0',
        borderBottom: '1px solid var(--color-border-light, #f0f0f0)',
      }}
    >
      {/* 标题行：图标 + 标签 + 一句话定义 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 }}>
        <span style={{ fontSize: 20, color: 'var(--color-primary, #1677ff)' }}>
          {concept.icon}
        </span>
        <div>
          <Title level={3} style={{ margin: 0 }}>{concept.label}</Title>
          <Text type="secondary">{concept.oneLiner}</Text>
        </div>
      </div>

      {/* 左右分栏：左栏字段表，右栏数据快照 */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '1fr 1fr',
          gap: 24,
          alignItems: 'start',
        }}
      >
        {/* 左栏：字段定义表 */}
        <div>
          <Text strong style={{ display: 'block', marginBottom: 8 }}>关键字段</Text>
          <Descriptions
            column={1}
            size="small"
            bordered
            items={concept.fields.map((f) => ({ label: f.name, children: f.desc }))}
          />
          {/* 特定概念附加对比表 */}
          {concept.id === 'executor' && <ExecutorVsExpertTable />}
          {concept.id === 'loop' && <TriggerTypesTable />}
          {(concept.id === 'loop' || concept.id === 'todo') && <GateTypesTable />}
        </div>

        {/* 右栏：数据快照或空态 */}
        <div>
          <Text strong style={{ display: 'block', marginBottom: 8 }}>数据快照</Text>
          {loading ? (
            <Spin style={{ display: 'block', margin: '24px auto' }} />
          ) : snapshot ? (
            <SnapshotView data={snapshot} />
          ) : (
            <EmptyState concept={concept} onGoto={handleGoto} />
          )}
        </div>
      </div>

      {/* 底部：跳转入口 */}
      <Button
        type="primary"
        icon={<ArrowRightOutlined />}
        onClick={handleGoto}
        style={{ marginTop: 16 }}
        data-testid={`onboarding-detail-goto-${concept.id}`}
      >
        去{concept.label}页
      </Button>
    </section>
  );
}

// 保留 import 引用占位（避免 tsc unused 警告，下方未直接用到的 API）。
void CONCEPTS;
void db;
