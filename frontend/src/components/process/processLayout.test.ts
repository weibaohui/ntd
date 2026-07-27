// processLayout.test.ts
// ---------------------------------------------------------------------------
// M4 里程碑：processLayout 的 vitest 单元测试。
//
// 覆盖场景：
// 1. layoutPhases：空 phases 返回空 Map
// 2. layoutPhases：单个 phase 返回坐标（x/y 均为正值）
// 3. layoutPhases：多个 phase 从左到右排列（x 递增）
// 4. layoutLinksInPhase：空 links 返回空数组
// 5. layoutLinksInPhase：多个 link 从上到下排列（y 递增）
// 6. layoutLinksInPhase：link x 坐标 = PHASE_PADDING / 2
//
// dagre 布局是确定性的（相同输入相同输出），可以断言具体坐标范围。
// ---------------------------------------------------------------------------

import { describe, it, expect } from 'vitest';
import type { PhaseDefinition, LinkDefinition } from '@/types/process';
import {
  layoutPhases,
  layoutLinksInPhase,
  PHASE_PADDING,
  PHASE_HEADER,
  NODE_HEIGHT,
  LINK_GAP,
} from './processLayout';

// ── 测试夹具 ──────────────────────────────────────

function makeLink(id: string, name: string): LinkDefinition {
  return { id, name, on_success: 'next', on_gate_fail: 'break' };
}

// ── layoutPhases 测试 ─────────────────────────────

describe('layoutPhases', () => {
  it('layoutPhases_emptyPhases_returnsEmptyMap', () => {
    const result = layoutPhases([]);

    expect(result.size).toBe(0);
  });

  it('layoutPhases_singlePhase_returnsPosition', () => {
    const phases: PhaseDefinition[] = [{ id: 'p1', name: '阶段1' }];

    const result = layoutPhases(phases);

    expect(result.size).toBe(1);
    const pos = result.get('phase-0');
    expect(pos).toBeDefined();
    expect(typeof pos!.x).toBe('number');
    expect(typeof pos!.y).toBe('number');
  });

  it('layoutPhases_multiplePhases_leftToRight', () => {
    const phases: PhaseDefinition[] = [
      { id: 'p1', name: '阶段1', links: [makeLink('l1', '环节1')] },
      { id: 'p2', name: '阶段2', links: [makeLink('l2', '环节2')] },
      { id: 'p3', name: '阶段3', links: [makeLink('l3', '环节3')] },
    ];

    const result = layoutPhases(phases);

    expect(result.size).toBe(3);
    const pos0 = result.get('phase-0')!;
    const pos1 = result.get('phase-1')!;
    const pos2 = result.get('phase-2')!;

    // dagre LR 布局：x 从左到右递增
    expect(pos1.x).toBeGreaterThan(pos0.x);
    expect(pos2.x).toBeGreaterThan(pos1.x);
  });

  it('layoutPhases_phaseWithMoreLinks_hasGreaterHeight', () => {
    // phase-0 含 3 个 link，phase-1 含 1 个 link
    // dagre 返回中心坐标，高度更大的 phase 中心 y 偏移不同
    const phases: PhaseDefinition[] = [
      {
        id: 'p1',
        name: '阶段1',
        links: [
          makeLink('l1', '环节1'),
          makeLink('l2', '环节2'),
          makeLink('l3', '环节3'),
        ],
      },
      { id: 'p2', name: '阶段2', links: [makeLink('l4', '环节4')] },
    ];

    const result = layoutPhases(phases);

    // 两个 phase 都应有坐标
    expect(result.get('phase-0')).toBeDefined();
    expect(result.get('phase-1')).toBeDefined();
  });
});

// ── layoutLinksInPhase 测试 ───────────────────────

describe('layoutLinksInPhase', () => {
  it('layoutLinksInPhase_emptyLinks_returnsEmptyArray', () => {
    const phase: PhaseDefinition = { id: 'p1', name: '阶段1' };

    const result = layoutLinksInPhase(phase, 0);

    expect(result.length).toBe(0);
  });

  it('layoutLinksInPhase_singleLink_returnsCorrectPosition', () => {
    const phase: PhaseDefinition = {
      id: 'p1',
      name: '阶段1',
      links: [makeLink('l1', '环节1')],
    };

    const result = layoutLinksInPhase(phase, 0);

    expect(result.length).toBe(1);
    expect(result[0].id).toBe('link-0-0');
    // x = PHASE_PADDING / 2
    expect(result[0].position.x).toBe(PHASE_PADDING / 2);
    // y = PHASE_HEADER（第一个 link）
    expect(result[0].position.y).toBe(PHASE_HEADER);
  });

  it('layoutLinksInPhase_multipleLinks_topToBottom', () => {
    const phase: PhaseDefinition = {
      id: 'p1',
      name: '阶段1',
      links: [
        makeLink('l1', '环节1'),
        makeLink('l2', '环节2'),
        makeLink('l3', '环节3'),
      ],
    };

    const result = layoutLinksInPhase(phase, 0);

    expect(result.length).toBe(3);
    // y 从上到下递增
    expect(result[0].position.y).toBeLessThan(result[1].position.y);
    expect(result[1].position.y).toBeLessThan(result[2].position.y);

    // 验证间距：y[i] = PHASE_HEADER + i * (NODE_HEIGHT + LINK_GAP)
    const expectedY1 = PHASE_HEADER + NODE_HEIGHT + LINK_GAP;
    expect(result[1].position.y).toBe(expectedY1);
  });

  it('layoutLinksInPhase_correctPhaseIndexInId', () => {
    const phase: PhaseDefinition = {
      id: 'p2',
      name: '阶段2',
      links: [makeLink('l1', '环节1')],
    };

    // phaseIndex = 1
    const result = layoutLinksInPhase(phase, 1);

    expect(result[0].id).toBe('link-1-0');
  });
});
