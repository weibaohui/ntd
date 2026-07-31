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
import { loopOptionLabel } from './constants';
import type { LoopLite } from './constants';

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

  // 防御分支：无工艺来源（类型上字段可选）时退化为 041 格式，不拼空括号。
  it('test_loopOptionLabel_无工艺来源_退化041格式', () => {
    const loop: LoopLite = { id: 21, name: '手工环路' };
    expect(loopOptionLabel(loop)).toBe('#21 手工环路');
  });
});
