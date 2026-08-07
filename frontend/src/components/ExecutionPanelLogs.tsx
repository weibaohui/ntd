// 091 性能优化：执行日志虚拟滚动列表。
//
// 历史问题：ExecutionPanel 直接 map 渲染全部日志 DOM 节点，长跑任务（数千行）
// 会生成数千个 DOM 节点，每次追加日志都触发整段 diff/重排，面板卡顿。
//
// 拆分后：用 @tanstack/react-virtual 只渲染可视区 + overscan 行，DOM 节点数恒定。
// 本组件订阅 LogsContext（useTaskLogs），因此只在日志变化时重渲染，
// 不影响 ExecutionPanel 的其他状态消费（执行态、tick 计时等）。
//
// 行高动态测量：日志内容长度不定会换行，固定估算会重叠/留白，故启用 measureElement。

import { memo, useEffect, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
// 091：total 选择器用于「共 N 条」截断提示——Sync 只回传最近 N 条，total 是全量条数。
import { useTaskLogs, useTaskLogTotal } from '@/hooks/useLogsContext';
import { LOG_TYPE_LABELS } from '@/constants';
import type { LogEntry } from '@/types';

/** 单行高度初估值：仅用于首屏占位，实际行高由 measureElement 校正。 */
const ESTIMATED_ROW_HEIGHT = 22;
/** 可视区外额外渲染的行数：平衡滚动流畅度与渲染量。 */
const OVERSCAN = 12;

interface ExecutionPanelLogsProps {
  /** 当前激活任务 id：变化时父组件用 key 强制重挂载，本组件据此取日志。 */
  taskId: string;
  /** 类型徽标颜色表：随主题切换的稳定模块常量。 */
  logTypeColors: Record<string, string>;
  /** 任务状态：finished 时追加结果块。 */
  status: 'running' | 'finished';
  /** 终态结果文本（成功/失败信息均在此）。 */
  result?: string | null;
  /** 终态成功标志：决定结果块配色。 */
  success?: boolean;
}

/** 把 ISO 时间格式化为 HH:mm:ss（本地时区），日志行时间戳专用。 */
function formatShortTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  } catch {
    return iso;
  }
}

/** 单行日志渲染：抽出以便虚拟列表内联调用，保持行级布局一致。 */
function LogRow({ log, color }: { log: LogEntry; color: string }) {
  return (
    <div className="log-line">
      <span className="log-timestamp">{formatShortTime(log.timestamp)}</span>
      <span
        className="log-type-badge"
        // `${color}20` 拼出 8 位十六进制 RGBA（20 ≈ 12% 透明度）作底色，与文字色同源。
        style={{ color, background: `${color}20` }}
      >
        {LOG_TYPE_LABELS[log.type] || log.type}
      </span>
      <span className="log-content">{log.content}</span>
    </div>
  );
}

export const ExecutionPanelLogs = memo(function ExecutionPanelLogs({
  taskId,
  logTypeColors,
  status,
  result,
  success,
}: ExecutionPanelLogsProps) {
  // 滚动容器：复用 .execution-panel-logs（flex:1 + overflow-y:auto）作为虚拟滚动视口。
  const parentRef = useRef<HTMLDivElement>(null);
  // 订阅 LogsContext 取本任务日志：仅日志变化触发重渲染，与 ExecutionPanel 执行态解耦。
  const logs = useTaskLogs(taskId);
  // 全量条数：Sync 给的历史快照条数 + 实时追加累加。logs 因 cap()/RECONNECT_LOG_CAP
  // 可能只保留最近一段，total > logs.length 即存在更早的截断历史，据此显示提示。
  const total = useTaskLogTotal(taskId);

  const virtualizer = useVirtualizer({
    count: logs.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ESTIMATED_ROW_HEIGHT,
    overscan: OVERSCAN,
    // 动态测量行高，适配多行换行的长日志内容，避免固定估算导致的重叠/留白。
    measureElement: el => el.getBoundingClientRect().height,
  });

  // 新日志到达自动滚到底：依赖 length，配合 align:'end' 钉住底部。
  // 高频追加下 smooth 滚动会抖动滞后，故用默认即时定位（对齐原 scrollIntoView 行为）。
  useEffect(() => {
    if (logs.length === 0) return;
    virtualizer.scrollToIndex(logs.length - 1, { align: 'end' });
  }, [logs.length, virtualizer]);

  // 终态到达时把容器滚到真正的底部：result 块位于虚拟 spacer 之后的正常流，
  // Finished 不改变 logs.length（上面的 length effect 不会触发），需单独把容器滚到
  // scrollHeight，才能让结果块进入视口，而不是停留在最后一行日志下方（091 评审修复）。
  useEffect(() => {
    if (status !== 'finished') return;
    const el = parentRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [status, result]);

  // 空日志早返回：放在所有 hook 之后，保证 hook 调用顺序稳定。
  if (logs.length === 0) {
    return (
      <div className="execution-panel-logs">
        {/* 空日志但有终态结果：任务可能在产出任何日志前就结束，此时展示结果而非「等待输出」。 */}
        {status === 'finished' && result ? (
          <div className={`log-result ${success ? 'log-result-success' : 'log-result-error'}`}>
            {result}
          </div>
        ) : (
          <div className="execution-panel-empty">等待输出...</div>
        )}
      </div>
    );
  }

  const items = virtualizer.getVirtualItems();
  // 仅当存在截断（全量条数 > 当前展示条数）时才提示，正常执行（未触顶）不显示，避免噪声。
  const showTruncationHint = total != null && total > logs.length;

  return (
    // 外层 wrap 纵向排「截断提示条 + 滚动容器」。提示条必须在滚动容器之外：
    // 若放进 .execution-panel-logs，会把虚拟列表的 spacer 下推 H 像素，破坏 translateY 偏移基准，
    // 导致 scrollToIndex 到底时少滚 H 像素（最后一行被裁切）。两者彻底分离即无此问题。
    <div className="execution-panel-logs-wrap">
      {showTruncationHint && (
        <div className="execution-panel-log-hint">
          {/* 告知用户：Sync 只回传最近一段，更早的历史需去执行详情页（已有分页）查看。 */}
          共 {total} 条 · 仅显示最近 {logs.length} 条，完整记录见执行详情
        </div>
      )}
      <div className="execution-panel-logs" ref={parentRef}>
        {/* 撑开滚动高度的相对定位容器：虚拟行绝对定位其内，由 translateY 定位到 vi.start。 */}
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative', width: '100%' }}>
          {items.map(vi => {
            const log = logs[vi.index];
            return (
              <div
                key={vi.key}
                // data-index 让 measureElement 回调知道测量的是哪一行，回填精确偏移。
                data-index={vi.index}
                ref={virtualizer.measureElement}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${vi.start}px)`,
                }}
              >
                <LogRow log={log} color={logTypeColors[log.type] || '#cbd5e1'} />
              </div>
            );
          })}
        </div>
        {/* 终态结果块：放在虚拟容器之后的正常流，随滚动自然出现在所有日志行末尾。 */}
        {status === 'finished' && result && (
          <div className={`log-result ${success ? 'log-result-success' : 'log-result-error'}`}>
            {result}
          </div>
        )}
      </div>
    </div>
  );
});
