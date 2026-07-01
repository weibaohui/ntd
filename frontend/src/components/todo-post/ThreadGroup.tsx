// 帖子组组件（所有记录平铺为连续楼层）。

import type { ExecutionRecord } from '@/types';
import type { SessionGroup } from './helpers';
import { PostCard } from './PostCard';
import { ReplyRow } from './ReplyRow';

interface ThreadGroupProps {
  group: SessionGroup;
  getNextFloor: () => number;
  onSelectRecord: (id: number) => void;
  onStop: (id: number) => Promise<void>;
  onReply: (r: ExecutionRecord, msg: string) => Promise<void>;
  replyLoading: boolean;
  onOpenLogDrawer: (id: number) => void;
  resolveExecutionStats: (r: ExecutionRecord, running: boolean) => any;
  todoTitle: string;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
  onExport: (record: ExecutionRecord) => Promise<void>;
}

/**
 * 帖子组组件：将同一 session 的所有记录平铺为连续楼层。
 * 每个记录渲染为 PostCard，最后一个记录下方渲染 ReplyRow。
 */
export function ThreadGroup({
  group,
  getNextFloor,
  onSelectRecord,
  onStop,
  onReply,
  replyLoading,
  onOpenLogDrawer,
  resolveExecutionStats,
  todoTitle,
  onRate,
  onExport,
}: ThreadGroupProps) {
  const allRecords = group.records;
  const lastRecord = allRecords[allRecords.length - 1];

  return (
    <div style={{ marginBottom: 24 }}>
      {allRecords.map((record, idx) => (
        <PostCard
          key={record.id}
          record={record}
          floor={getNextFloor()}
          isContinuation={idx > 0}
          onSelect={() => onSelectRecord(record.id)}
          onStop={onStop}
          onOpenLogDrawer={onOpenLogDrawer}
          resolveExecutionStats={resolveExecutionStats}
          todoTitle={todoTitle}
          onRate={onRate}
          onExport={onExport}
        />
      ))}
      <ReplyRow record={lastRecord} onReply={onReply} loading={replyLoading} />
    </div>
  );
}
