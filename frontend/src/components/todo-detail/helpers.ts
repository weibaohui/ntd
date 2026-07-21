import type { ExecutionRecord } from '@/types';

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

// logTypeColors 和 logTypeLabels 已迁移到 @/constants
// 保留 getElapsedSeconds / groupBySession 等工具函数

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
