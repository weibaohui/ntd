// 回复输入行组件。

import type { ExecutionRecord } from '@/types';
import { ReplyInput } from '../todo-detail/ReplyInput';

interface ReplyRowProps {
  record: ExecutionRecord;
  onReply: (r: ExecutionRecord, msg: string) => Promise<void>;
  loading: boolean;
}

/**
 * 回复输入行组件，仅在记录状态不是 running 且支持 resume 时渲染。
 */
export function ReplyRow({ record, onReply, loading }: ReplyRowProps) {
  // 仅在 running 状态或不支持 resume 时不渲染
  if (record.status === 'running' || !record.resume_message) return null;
  return (
    <div style={{ padding: "4px 0" }}>
      <ReplyInput record={record} onReply={onReply} loading={loading} />
    </div>
  );
}
