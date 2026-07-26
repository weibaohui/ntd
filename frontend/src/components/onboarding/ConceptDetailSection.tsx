// 概念详细说明区：每个概念一段。
// 字段定义表（Descriptions）+ 特定概念附加对比表。

import { Typography, Descriptions, Tag, Table } from 'antd';
import {
  EXECUTOR_VS_EXPERT_VS_MODEL,
  TRIGGER_TYPES,
  GATE_TYPES,
  type ConceptNode,
} from '@/components/onboarding/concepts';

const { Text, Title } = Typography;

interface ConceptDetailSectionProps {
  concept: ConceptNode;
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
export function ConceptDetailSection({ concept }: ConceptDetailSectionProps) {
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

      {/* 字段定义表（无标题字，直接展示） */}
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
    </section>
  );
}

