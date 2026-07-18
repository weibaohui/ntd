// 执行详情的可折叠小节：子 Agent 列表 + 待办进度列表。
// 这两类都是「执行过程中产生的结构化信息」，此前只采集不展示（多 Agent 显示能力补齐）。
// 数据来源：record.agent_runs / record.todo_progress（后端写入的 JSON 字符串，前端 parse）。

import { useState, type ReactNode } from 'react';
import { Tag } from 'antd';
import {
  CaretDownOutlined,
  CaretUpOutlined,
  RobotOutlined,
  CheckCircleFilled,
  ClockCircleOutlined,
  MinusOutlined,
} from '@ant-design/icons';
import type { ExecutionRecord, AgentRun } from '@/types';
import type { TodoItem } from '@/types/todo';
import { formatLocalDateTime } from '@/utils/datetime';

/** 通用可折叠小节：头部（展开箭头 + 图标 + 标题 + 计数）+ 展开后正文。默认展开。 */
function CollapsibleSection({
  icon,
  title,
  count,
  children,
}: {
  icon: ReactNode;
  title: string;
  count: number;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(true);
  // stopPropagation：避免点折叠按钮时冒泡到 PostCard 根的 onSelect（误选帖子）。
  const toggle = (e: React.MouseEvent) => {
    e.stopPropagation();
    setOpen((o) => !o);
  };
  return (
    <div style={{ marginBottom: 10 }}>
      <button
        type="button"
        onClick={toggle}
        aria-expanded={open}
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: 6,
          padding: '2px 8px',
          background: 'none',
          border: 'none',
          cursor: 'pointer',
          color: 'var(--color-text)',
        }}
      >
        {open ? <CaretUpOutlined style={{ fontSize: 11 }} /> : <CaretDownOutlined style={{ fontSize: 11 }} />}
        {icon}
        <span style={{ fontSize: 13, fontWeight: 600 }}>{title}</span>
        <Tag style={{ margin: 0, fontSize: 11 }}>{count}</Tag>
      </button>
      {open && <div style={{ marginTop: 6 }}>{children}</div>}
    </div>
  );
}

/** 把后端 JSON 字符串字段安全解析成数组；非法或非数组时返回空数组。 */
function parseJsonArray<T>(raw: string | null | undefined): T[] {
  if (!raw) return [];
  try {
    const v = JSON.parse(raw);
    return Array.isArray(v) ? (v as T[]) : [];
  } catch {
    return [];
  }
}

const AGENT_STATUS_COLOR: Record<string, string> = {
  completed: 'var(--color-success)',
  failed: 'var(--color-error)',
  running: 'var(--color-info)',
};

/** 子 Agent 列表小节：仅当 record.agent_runs 解析出非空列表时渲染。 */
export function AgentRunsSection({ record }: { record: ExecutionRecord }) {
  const agents = parseJsonArray<AgentRun>(record.agent_runs);
  if (agents.length === 0) return null;
  return (
    <CollapsibleSection
      icon={<RobotOutlined />}
      title="子 Agent"
      count={agents.length}
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        {agents.map((a, i) => (
          <div
            key={`${a.name}-${i}`}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              padding: '4px 8px',
              background: 'var(--color-bg-base)',
              borderRadius: 6,
              flexWrap: 'wrap',
            }}
          >
            <span style={{ fontSize: 12, fontWeight: 600, color: 'var(--color-text)' }}>{a.name}</span>
            {a.role && (
              <Tag color="blue" style={{ margin: 0, fontSize: 11 }}>
                {a.role}
              </Tag>
            )}
            <span style={{ fontSize: 11, color: AGENT_STATUS_COLOR[a.status] ?? 'var(--color-text-tertiary)' }}>
              {a.status}
            </span>
            {a.started_at && (
              <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>
                {formatLocalDateTime(a.started_at)}
              </span>
            )}
          </div>
        ))}
      </div>
    </CollapsibleSection>
  );
}

/** 待办进度状态 → 图标映射；后端归一化的 cancelled/failed/running 走兜底图标。 */
const TODO_ICON: Record<string, ReactNode> = {
  completed: <CheckCircleFilled style={{ color: 'var(--color-success)' }} />,
  in_progress: <ClockCircleOutlined style={{ color: 'var(--color-info)' }} />,
  pending: <MinusOutlined style={{ color: 'var(--color-text-tertiary)' }} />,
};

/** 待办进度小节：仅当 record.todo_progress 解析出非空列表时渲染。 */
export function TodoProgressSection({ record }: { record: ExecutionRecord }) {
  const todos = parseJsonArray<TodoItem>(record.todo_progress);
  if (todos.length === 0) return null;
  const done = todos.filter((t) => t.status === 'completed').length;
  return (
    <CollapsibleSection icon={<span>📝</span>} title="待办进度" count={todos.length}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        {todos.map((t, i) => (
          <div key={t.id ?? i} style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12 }}>
            {TODO_ICON[t.status] ?? <MinusOutlined style={{ color: 'var(--color-text-tertiary)' }} />}
            <span
              style={{
                textDecoration: t.status === 'completed' ? 'line-through' : 'none',
                color: t.status === 'completed' ? 'var(--color-text-tertiary)' : 'var(--color-text)',
              }}
            >
              {t.content}
            </span>
          </div>
        ))}
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 2 }}>
          已完成 {done}/{todos.length}
        </div>
      </div>
    </CollapsibleSection>
  );
}
