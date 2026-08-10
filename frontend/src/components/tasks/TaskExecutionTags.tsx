// NTD-013 / CodeRabbit Opinion 2：三视图（Table/Kanban/Card）的「执行方式徽标」与
// 「工艺/委派信息 Tag」原本在各视图各写一份，委派文案一度漂移成「委派：…🚀接力」。
// 这里收口为唯一事实源，杜绝再次漂移——任何视图改文案只需改这一处。
//
// 设计取舍（YAGNI）：
//   - 委派 Tag 文案是真正被复用、且历史上漂移过的部分 → 组件统一渲染（防漂移收益在此）。
//   - 环路分支三视图本就不同（Table 走三段式 #工艺id-名称-版本，Kanban/Card 仅显示工艺名），
//     强行统一反而制造假抽象 → 由调用方经 loopTag 传入各自渲染，组件只负责委派分支。

import type { CSSProperties } from 'react';
import { Tag, Typography } from 'antd';
import type { TaskItem } from '@/components/tasks/constants';

const { Text } = Typography;

/** 环路/委派类型徽标：状态 Tag 旁的极小灰字提示，三视图状态行共用。
 *  默认 fontSize 10；调用方可经 style 覆盖（如 Table 状态单元需 marginLeft:4 对齐）。 */
export function ExecutionModeBadge({ mode, style }: { mode?: string; style?: CSSProperties }) {
  return (
    <Text type="secondary" style={{ fontSize: 10, ...style }}>
      {mode === 'delegate' ? '委派' : '环路'}
    </Text>
  );
}

/**
 * 工艺 / 委派信息 Tag：按 execution_mode 分支。
 *
 * - delegate：蓝色「委派给：<处理人>（专家/执行器）🚀自动接力」——三视图唯一文案口径，
 *   delegateStyle 控制各视图所需的外边距/字号（Table 默认、Kanban/Card 各异）。
 * - 环路：渲染调用方传入的 loopTag（Table 三段式 / Kanban/Card 工艺名），无则不渲染。
 */
export function TaskExecutionInfoTag({
  task,
  loopTag,
  delegateStyle,
}: {
  task: TaskItem;
  /** 环路模式下要渲染的节点（各视图口径不同，由调用方决定）。 */
  loopTag?: React.ReactNode;
  /** 委派 Tag 的样式覆盖（外边距/字号，三视图不同）。 */
  delegateStyle?: CSSProperties;
}) {
  // 委派分支：文案唯一收口，assignee 缺失兜底「未知处理人」防空值渲染。
  if (task.execution_mode === 'delegate') {
    const kindLabel = task.assignee_kind === 'expert' ? '专家' : '执行器';
    const name = task.assignee_name?.trim() || '未知处理人';
    const suffix = task.auto_continue ? ' 🚀自动接力' : '';
    return (
      <Tag color="blue" style={delegateStyle}>
        {`委派给：${name}（${kindLabel}）${suffix}`}
      </Tag>
    );
  }
  // 环路分支：透传调用方节点；antd Tag 由调用方决定（含 margin/字号差异）。
  return <>{loopTag ?? null}</>;
}
