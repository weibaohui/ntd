import type { ExecutionRecord } from '../../types';

/** 计算从 started_at 到现在的 elapsed time (秒) */
export function getElapsedSeconds(startedAt: string): number {
  const start = new Date(startedAt).getTime();
  const now = Date.now();
  return Math.floor((now - start) / 1000);
}

/** 按 session_id 分组执行记录，同一 session 的记录按时间排序形成链 */
export interface SessionGroup {
  sessionId: string;
  records: ExecutionRecord[];
}

export function groupBySession(records: ExecutionRecord[]): SessionGroup[] {
  const map = new Map<string, ExecutionRecord[]>();
  for (const r of records) {
    const key = r.session_id || `__single_${r.id}`;
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(r);
  }
  const groups: SessionGroup[] = [];
  for (const [sessionId, recs] of map) {
    recs.sort((a, b) => (a.started_at || '').localeCompare(b.started_at || ''));
    groups.push({ sessionId, records: recs });
  }
  groups.sort((a, b) => {
    const aLatest = a.records[a.records.length - 1].started_at || '';
    const bLatest = b.records[b.records.length - 1].started_at || '';
    return bLatest.localeCompare(aLatest);
  });
  return groups;
}

export function hasLogsStatic(record: ExecutionRecord): boolean {
  return record.status !== 'running' && !!record.finished_at;
}

export const logTypeColors: Record<string, string> = {
  info: '#60a5fa',
  text: '#4ade80',
  tool: '#fbbf24',
  tool_use: '#fbbf24',
  tool_call: '#fbbf24',
  tool_result: '#fbbf24',
  step_start: '#c084fc',
  step_finish: '#2dd4bf',
  stdout: '#cbd5e1',
  stderr: '#94a3b8',
  error: '#ef4444',
  system: '#94a3b8',
  assistant: '#a78bfa',
  user: '#22d3ee',
  result: '#4ade80',
  thinking: '#fb923c',
  tokens: '#94a3b8',
};

export const logTypeLabels: Record<string, string> = {
  info: 'INFO',
  text: 'TEXT',
  tool: 'TOOL',
  tool_use: 'TOOL',
  tool_call: 'TOOL',
  tool_result: 'RESULT',
  step_start: 'START',
  step_finish: 'END',
  stdout: 'OUT',
  stderr: 'LOG',
  error: 'ERROR',
  system: 'SYS',
  assistant: 'ASST',
  user: 'USER',
  result: 'RESULT',
  thinking: 'THINK',
  tokens: 'INFO',
};

/**
 * 格式化时间戳为短时间格式 (HH:mm:ss)
 */
export function formatLogTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  } catch {
    return iso;
  }
}
