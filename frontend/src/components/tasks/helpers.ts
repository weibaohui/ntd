// 任务详情-执行历史的时间信息格式化辅助。
//
// 为什么单独抽这个文件：
// 执行历史行的时间展示涉及「UTC ISO → 本地时间」「起止差值 → 人类可读耗时」
// 两段纯逻辑，抽成纯函数后可以脱离组件做单元测试（vitest），
// 也让 TaskDetailPanel.tsx 的 JSX 保持声明式、不掺计算细节。

import { formatDateTime, formatDurationSec } from '@/utils/format';

/**
 * 计算执行耗时（秒）。
 *
 * 返回 null 表示「无法得出有意义的耗时」，调用方应直接不显示耗时部分：
 * - 任一时间缺失（进行中的执行没有 finished_at）；
 * - 时间字符串无法解析（防御后端异常数据，避免 NaN 传播到 UI）；
 * - 出现负耗时（时钟回拨/数据异常时显示负数会误导用户）。
 */
export function execDurationSec(
  startedAt: string | null | undefined,
  finishedAt: string | null | undefined,
): number | null {
  // 进行中的执行没有 finished_at，此时谈不上「耗时」，进行中状态由状态 Tag 表达。
  if (!startedAt || !finishedAt) return null;
  const startMs = new Date(startedAt).getTime();
  const endMs = new Date(finishedAt).getTime();
  // 防御非法时间字符串：NaN 参与运算会让整个显示串变成 NaN，必须提前拦截。
  if (Number.isNaN(startMs) || Number.isNaN(endMs)) return null;
  const sec = Math.round((endMs - startMs) / 1000);
  // 负耗时属于数据异常（时钟回拨等），显示「-5s」比不显示更误导用户。
  if (sec < 0) return null;
  return sec;
}

/**
 * 组装执行历史行的时间信息文本。
 *
 * 输出形态：
 * - 已结束：`2026/7/4 20:15 开始 · 耗时 1m54s`
 * - 进行中（无 finished_at）：`2026/7/4 20:15 开始`
 * - 无开始时间：空串（schema 上 started_at 非空，此处纯防御，调用方判空不渲染）
 *
 * 开始时间用 formatDateTime（分钟精度）：执行历史是列表场景，
 * 秒级精度只增加视觉噪音；需要精确时间可展开执行看板查看。
 * 耗时复用项目统一的 formatDurationSec，与 RunningRecordDrawer 等处口径一致。
 */
export function formatExecTimeInfo(
  startedAt: string | null | undefined,
  finishedAt: string | null | undefined,
): string {
  // started_at 是时间信息的锚点，缺失时整段不显示（返回空串由调用方跳过渲染）。
  if (!startedAt) return '';
  const parts = [`${formatDateTime(startedAt)} 开始`];
  // 耗时可计算时才追加；进行中或异常数据只保留开始时间。
  const durationSec = execDurationSec(startedAt, finishedAt);
  if (durationSec !== null) {
    parts.push(`耗时 ${formatDurationSec(durationSec)}`);
  }
  return parts.join(' · ');
}
