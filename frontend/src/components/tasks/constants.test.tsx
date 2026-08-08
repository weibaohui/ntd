// constants.test.tsx
// ---------------------------------------------------------------------------
// 049：新建任务下拉选项文案 loopOptionLabel 的纯函数测试。
//
// 目标格式：`#<环路ID> 环路名称（#工艺ID 工艺名称 工艺版本）`，
// 回退口径与 utils/processText 的 formatProcessText 对齐：
//   名称 display_name → name → `#<工艺ID>`；版本缺失用 '—'；
//   无工艺来源退化为 041 格式 `#<环路ID> <名称>`。
// ---------------------------------------------------------------------------

import { describe, it, expect } from 'vitest';
import {
  PENDING_APPROVAL_LANE,
  TASK_LANES,
  TASK_STATUS_FILTER_OPTIONS,
  isPendingApproval,
  laneOfTask,
  loopOptionLabel,
  matchesTaskStatusFilter,
} from './constants';
import type { LoopLite, TaskItem } from './constants';

describe('loopOptionLabel', () => {
  // 正常路径：全字段齐备时优先使用 display_name（中文名），版本原样展示。
  it('test_loopOptionLabel_全字段齐备_完整格式且display_name优先', () => {
    const loop: LoopLite = {
      id: 12,
      name: '代码评审环路',
      process_template_id: 3,
      process_template_display_name: '标准需求交付',
      process_template_name: '4p12s-delivery',
      process_template_version: '1.2.0',
    };
    expect(loopOptionLabel(loop)).toBe('#12 代码评审环路（#3 标准需求交付 1.2.0）');
  });

  // 回退路径：display_name 缺失时用标识名 process_template_name。
  it('test_loopOptionLabel_displayName缺失_回退标识名', () => {
    const loop: LoopLite = {
      id: 7,
      name: '交付环路',
      process_template_id: 3,
      process_template_name: '4p12s-delivery',
      process_template_version: '2.0.0',
    };
    expect(loopOptionLabel(loop)).toBe('#7 交付环路（#3 4p12s-delivery 2.0.0）');
  });

  // 边界：版本缺失/空白时用 '—' 占位，保持括号内三段式结构。
  it('test_loopOptionLabel_版本缺失_占位符兜底', () => {
    const loop: LoopLite = {
      id: 9,
      name: '评审环路',
      process_template_id: 5,
      process_template_display_name: '轻量评审',
      process_template_version: null,
    };
    expect(loopOptionLabel(loop)).toBe('#9 评审环路（#5 轻量评审 —）');
  });

  // 回退路径：display_name 为纯空白串时不能占位，应继续回退到有效的标识名
  // （修复前 rawName ?? 链会在 trim 失败后直接兜底 #id，跳过有效 name）。
  it('test_loopOptionLabel_displayName空白_继续回退标识名', () => {
    const loop: LoopLite = {
      id: 15,
      name: '交付环路',
      process_template_id: 3,
      process_template_display_name: '   ',
      process_template_name: '4p12s-delivery',
      process_template_version: '1.0.0',
    };
    expect(loopOptionLabel(loop)).toBe('#15 交付环路（#3 4p12s-delivery 1.0.0）');
  });

  // 防御分支：无工艺来源（类型上字段可选）时退化为 041 格式，不拼空括号。
  it('test_loopOptionLabel_无工艺来源_退化041格式', () => {
    const loop: LoopLite = { id: 21, name: '手工环路' };
    expect(loopOptionLabel(loop)).toBe('#21 手工环路');
  });
});

// 063：任务待审批透出 —— 泳道定义 / 待审批判定 / 看板分组优先级。
describe('待审批泳道与分组（063）', () => {
  // 构造最小 TaskItem：只填本组测试关心的字段，其余置空。
  const makeTask = (over: Partial<TaskItem>): TaskItem => ({
    id: 1,
    title: 't',
    description: '',
    status: 'running',
    ...over,
  });

  // 「待审批」泳道必须存在且为第一列：需要人处理的事项进页即见。
  it('test_TASK_LANES_待审批泳道位于首列', () => {
    expect(TASK_LANES[0].status).toBe(PENDING_APPROVAL_LANE);
    expect(TASK_LANES[0].label).toBe('待审批');
  });

  // 待审批判定：>0 成立；0 / undefined（老数据无字段）不成立。
  it('test_isPendingApproval_计数边界', () => {
    expect(isPendingApproval(makeTask({ pending_approval_count: 2 }))).toBe(true);
    expect(isPendingApproval(makeTask({ pending_approval_count: 1 }))).toBe(true);
    expect(isPendingApproval(makeTask({ pending_approval_count: 0 }))).toBe(false);
    expect(isPendingApproval(makeTask({}))).toBe(false);
  });

  // 分组优先级：待审批任务只进待审批泳道，不再落入真实 status 泳道，避免看板计数翻倍。
  it('test_laneOfTask_待审批优先于真实状态', () => {
    expect(laneOfTask(makeTask({ status: 'running', pending_approval_count: 3 })))
      .toBe(PENDING_APPROVAL_LANE);
    expect(laneOfTask(makeTask({ status: 'failed', pending_approval_count: 1 })))
      .toBe(PENDING_APPROVAL_LANE);
  });

  // 无待审批时按真实 status 分组，行为与 063 之前一致。
  it('test_laneOfTask_无待审批回退真实状态', () => {
    expect(laneOfTask(makeTask({ status: 'success', pending_approval_count: 0 }))).toBe('success');
    expect(laneOfTask(makeTask({ status: 'pending' }))).toBe('pending');
  });
});

// 063 PR 评审收口：状态筛选项与过滤谓词为列表/卡片视图共享的唯一事实源。
describe('状态筛选共享口径（063 评审收口）', () => {
  const makeTask = (over: Partial<TaskItem>): TaskItem => ({
    id: 1,
    title: 't',
    description: '',
    status: 'running',
    ...over,
  });

  // 筛选项必须包含「待审批」虚拟项——两视图渲染同一份数组，不会出现一处有一处无。
  it('test_TASK_STATUS_FILTER_OPTIONS_含待审批虚拟项', () => {
    const values = TASK_STATUS_FILTER_OPTIONS.map((o) => o.value);
    expect(values).toContain(PENDING_APPROVAL_LANE);
    expect(values).toEqual(['all', PENDING_APPROVAL_LANE, 'pending', 'running', 'success', 'failed']);
  });

  // all 不筛；真实状态精确匹配；待审批虚拟项按计数判定（含 0 与 undefined 边界）。
  it('test_matchesTaskStatusFilter_all不筛选', () => {
    expect(matchesTaskStatusFilter(makeTask({ status: 'failed' }), 'all')).toBe(true);
  });

  it('test_matchesTaskStatusFilter_真实状态精确匹配', () => {
    expect(matchesTaskStatusFilter(makeTask({ status: 'running' }), 'running')).toBe(true);
    expect(matchesTaskStatusFilter(makeTask({ status: 'running' }), 'failed')).toBe(false);
  });

  it('test_matchesTaskStatusFilter_待审批虚拟项按计数过滤', () => {
    expect(matchesTaskStatusFilter(makeTask({ pending_approval_count: 2 }), PENDING_APPROVAL_LANE)).toBe(true);
    expect(matchesTaskStatusFilter(makeTask({ pending_approval_count: 0 }), PENDING_APPROVAL_LANE)).toBe(false);
    expect(matchesTaskStatusFilter(makeTask({}), PENDING_APPROVAL_LANE)).toBe(false);
  });
});
