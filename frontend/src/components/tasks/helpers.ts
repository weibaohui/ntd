// 任务详情面板（TaskDetailPanel）的纯展示逻辑辅助。
//
// 为什么单独抽这个文件：
// 详情页里有两类「计算」逻辑——执行历史行的时间格式化、Tab 的显隐与选中态——
// 它们都与组件渲染/数据加载解耦，抽成纯函数后可以脱离组件做单元测试（vitest），
// 也让 TaskDetailPanel.tsx 的 JSX 保持声明式、不掺计算细节。
// 时间相关：execDurationSec / formatExecTimeInfo。
// Tab 显隐（需求 093）：TaskTabKey / visibleTaskTabs / resolveTaskActiveTab。

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

// ===== 任务详情 Tab 显隐与选中态（需求 093）=====

// 任务详情的 4 个 Tab key，按展示顺序排列；与 TaskDetailPanel 的 allTabs 定义顺序一致。
// 单一来源：显隐集合与 URL ?tab= 校验都以此为口径，避免散落字符串字面量。
export type TaskTabKey = 'overview' | 'dag' | 'exec' | 'discussion';

/**
 * 按执行方式返回「应展示」的 Tab key 集合（有序）。
 *
 * 「执行环路(dag)」「执行历史(exec)」强依赖 task.loop_id：仅工艺环路任务
 * （execution_mode==='loop'）才绑定环路、才有这两个 Tab 的数据；委派任务
 * （execution_mode==='delegate'）不建环路，展示这两个 Tab 只会渲染空状态
 * （无意义且误导），故隐藏。历史无 execution_mode 字段的旧任务按全部展示（与
 * 「非委派即展示」的既有口径一致，DB 默认值为 loop）。
 */
export function visibleTaskTabs(executionMode: string | undefined): TaskTabKey[] {
  // 委派任务隐藏 dag/exec，只留概览与讨论。
  if (executionMode === 'delegate') return ['overview', 'discussion'];
  // 工艺环路（loop）或历史无字段旧任务：4 个 Tab 全展示。
  return ['overview', 'dag', 'exec', 'discussion'];
}

/**
 * 解析当前生效的 Tab key：URL ?tab= 偏好命中可见集合则采纳，否则回退默认。
 *
 * 偏好可能因执行方式被隐藏——例如委派任务残留的 ?tab=dag 链接。Ant Design Tabs 若
 * activeKey 指向不存在的 Tab 会落到无选中态、内容区空白。这里据可见集合校验：
 * 偏好命中可见集合才采纳；否则回退默认 Tab（委派→讨论，其余→概览），与详情页
 * 既有默认 Tab 口径一致。preferred 非 Tab key（如 ?tab=garbage）同样回退。
 */
export function resolveTaskActiveTab(
  preferred: string | undefined,
  executionMode: string | undefined,
): TaskTabKey {
  const visible = visibleTaskTabs(executionMode);
  // preferred 命中可见集合才采纳；includes 需要 string，TaskTabKey 本身即 string 子集。
  if (preferred && (visible as readonly string[]).includes(preferred)) {
    return preferred as TaskTabKey;
  }
  // 回退默认 Tab：委派任务无概览之外有意义的首屏，进讨论区承接首次执行（与 060 @ 机制衔接）。
  return executionMode === 'delegate' ? 'discussion' : 'overview';
}
