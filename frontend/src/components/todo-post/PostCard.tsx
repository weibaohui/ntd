// 主帖卡片组件：展示单条执行记录的完整信息。

import { useState, useEffect } from 'react';
import { Button, Tag } from 'antd';
import {
  StopOutlined,
  InfoCircleOutlined,
  LinkOutlined,
  FileTextOutlined,
} from '@ant-design/icons';
import type { ExecutionRecord } from '@/types';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { CollapsibleConclusion } from '../todo-detail/CollapsibleConclusion';
import { CollapsibleCommand } from './CollapsibleCommand';
import { WorktreePathDisplay } from './WorktreePathDisplay';
import { RatingControl } from './RatingControl';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';
import { getElapsedSeconds } from './helpers';

interface PostCardProps {
  record: ExecutionRecord;
  floor: number;
  isContinuation?: boolean;
  onSelect: () => void;
  onStop: (id: number) => Promise<void>;
  onOpenLogDrawer: (id: number) => void;
  resolveExecutionStats: (r: ExecutionRecord, running: boolean) => any;
  todoTitle?: string;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
  onExport: (record: ExecutionRecord) => Promise<void>;
}

export function PostCard({
  record,
  floor,
  isContinuation = false,
  onSelect,
  onStop,
  onOpenLogDrawer,
  resolveExecutionStats,
  todoTitle,
  onRate,
  onExport,
}: PostCardProps) {
  const isRunning = record.status === "running";
  const [elapsedSec, setElapsedSec] = useState(
    isRunning ? getElapsedSeconds(record.started_at) : 0
  );

  useEffect(() => {
    if (!isRunning) return;
    const tick = () => setElapsedSec(getElapsedSeconds(record.started_at));
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [isRunning, record.started_at]);

  const stats = resolveExecutionStats(record, isRunning);

  return (
    <div
      onClick={onSelect}
      style={{
        background: "var(--color-bg-elevated)",
        border: "1px solid var(--color-border-light)",
        borderRadius: 8,
        padding: "16px 20px",
        cursor: "pointer",
        marginBottom: 2,
      }}
    >
      {/* 帖子头：楼号、标题、状态和操作按钮 */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 10,
          paddingBottom: 8,
          borderBottom: "1px dashed var(--color-border-light)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10, flex: 1, minWidth: 0 }}>
          <span style={{ fontSize: 14, fontWeight: 700, color: "var(--color-primary)", flexShrink: 0 }}>
            #{floor}
          </span>
          {isContinuation && <LinkOutlined style={{ fontSize: 12, color: "var(--color-primary)", flexShrink: 0 }} />}
          <span style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--color-text)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}>
            {isContinuation
              ? (record.resume_message
                  ? String(record.resume_message)
                  : "继续对话")
              : todoTitle || "初始执行"}
          </span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              fontSize: 11,
              padding: "2px 10px",
              borderRadius: 12,
              backgroundColor:
                record.status === "success"
                  ? "var(--color-success)"
                  : record.status === "failed"
                  ? "var(--color-error)"
                  : "var(--color-info)",
              color: "#fff",
              fontWeight: 600,
            }}
          >
            {record.status === "success" ? "成功" : record.status === "failed" ? "失败" : "进行中"}
          </span>
          {isRunning && (
            <Button
              type="text" size="small" danger icon={<StopOutlined />}
              onClick={(e) => { e.stopPropagation(); onStop(record.id); }}
            >
              停止
            </Button>
          )}
          <Button
            type="text" size="small" icon={<InfoCircleOutlined />}
            onClick={(e) => { e.stopPropagation(); onOpenLogDrawer(record.id); }}
          >
            详情
          </Button>
        </div>
      </div>

      {/* 帖子内容 —— 结论 */}
      {record.result ? (
        <CollapsibleConclusion result={record.result} status={record.status} recordId={record.id} />
      ) : isRunning ? (
        <div style={{ color: "var(--color-text-tertiary)", fontSize: 13, padding: "8px 0" }}>
          执行中...
        </div>
      ) : (
        <div style={{ color: "var(--color-text-tertiary)", fontSize: 13, padding: "8px 0" }}>
          暂无结论
        </div>
      )}

      {/* worktree 路径：仅当 record.worktree_path 非空时渲染 */}
      <WorktreePathDisplay worktreePath={record.worktree_path ?? null} />

      {/* 命令（可折叠，与结论样式一致）—— 从详情抽屉迁移到结论下方 */}
      {record.command && (
        <CollapsibleCommand command={record.command} title="命令" />
      )}

      {/* 元信息 + 操作：执行器、时间、触发类型、评分、导出、统计 */}
      <div style={{
        display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap",
        marginTop: 8, fontSize: 12,
      }}>
        {record.executor && <ExecutorBadge executor={record.executor} />}
        {record.model && (
          <Tag color="#3b82f6" style={{ margin: 0, fontSize: 11 }}>
            {record.model}
          </Tag>
        )}
        <span style={{ color: "var(--color-text-tertiary)" }}>
          {formatLocalDateTime(record.started_at)}
        </span>
        <Tag
          color={
            record.trigger_type === "cron"
              ? "#8b5cf6"
              : record.trigger_type?.startsWith("hook:")
              ? "#a855f7"
              : "#6b7280"
          }
          style={{ margin: 0, fontSize: 11 }}
        >
          {record.trigger_type === "cron"
            ? "Cron"
            : record.trigger_type?.startsWith("hook:")
            ? "Hook"
            : "手动"}
        </Tag>
        {!isRunning && record.usage?.duration_ms && (
          <span style={{ color: "var(--color-success)", fontWeight: 600 }}>
            {formatDurationSec(record.usage.duration_ms / 1000)}
          </span>
        )}
        {isRunning && elapsedSec > 0 && (
          <span style={{ color: "var(--color-info)", fontWeight: 600 }}>
            {formatDurationSec(elapsedSec)}
          </span>
        )}
        {/* 评分组件 */}
        {!isRunning && (
          <span onClick={(e) => e.stopPropagation()}>
            <RatingControl record={record} onRate={onRate} />
          </span>
        )}
        {/* 导出YAML按钮 */}
        {!isRunning && !!record.finished_at && (
          <Button type="text" size="small" icon={<FileTextOutlined />} onClick={(e) => { e.stopPropagation(); onExport(record); }}>
            导出YAML
          </Button>
        )}
      </div>

      {/* 统计 */}
      {record.usage && stats && (
        <div style={{
          display: "flex", gap: 16, marginTop: 8, fontSize: 11,
          color: "var(--color-text-tertiary)", flexWrap: "wrap",
        }}>
          {record.usage && (
            <>
              <span>Input: <b>{record.usage.input_tokens.toLocaleString()}</b></span>
              <span>Output: <b>{record.usage.output_tokens.toLocaleString()}</b></span>
              {record.usage.cache_read_input_tokens != null && record.usage.cache_read_input_tokens > 0 && (
                <span>缓存读: <b>{record.usage.cache_read_input_tokens.toLocaleString()}</b></span>
              )}
              {record.usage.cache_creation_input_tokens != null && record.usage.cache_creation_input_tokens > 0 && (
                <span>缓存写: <b>{record.usage.cache_creation_input_tokens.toLocaleString()}</b></span>
              )}
              {record.usage.total_cost_usd != null && (
                <span style={{ color: "var(--color-warning)" }}>
                  ${record.usage.total_cost_usd.toFixed(6)}
                </span>
              )}
            </>
          )}
          {stats && (
            <>
              <span>工具调用: <b style={{ color: "var(--color-primary)" }}>{stats.tool_calls}</b></span>
              <span>对话轮次: <b style={{ color: "var(--color-primary)" }}>{stats.conversation_turns}</b></span>
            </>
          )}
        </div>
      )}
    </div>
  );
}
