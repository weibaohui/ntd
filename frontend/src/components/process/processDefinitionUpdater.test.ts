// processDefinitionUpdater.test.ts
// ---------------------------------------------------------------------------
// M4 里程碑：processDefinitionUpdater 的 vitest 单元测试。
//
// 覆盖场景：
// 1. addPhase：空 phases 时追加
// 2. removePhase：级联重置悬空 goto
// 3. addLink：向指定 phase 追加 link
// 4. removeLink：级联重置悬空 goto
// 5. updateLinkField：修改 link 字段
// 6. updatePhaseField：修改 phase 字段
// 7. setLinkGoto：拖连线后设置 goto
// 8. resetLinkGoto：删除连线后重置为默认值
// 9. findGotoReferrers：查找所有引用指定 link 的 goto
// 10. findGotoReferrersForPhase：查找所有引用指定 phase 下 link 的 goto
//
// 不可变更新验证：所有更新函数返回新对象，原对象不变。
// ---------------------------------------------------------------------------

import { describe, it, expect } from 'vitest';
import type { ProcessDefinition, PhaseDefinition, LinkDefinition } from '@/types/process';
import {
  addPhase,
  removePhase,
  addLink,
  removeLink,
  updateLinkField,
  updatePhaseField,
  setLinkGoto,
  resetLinkGoto,
  findGotoReferrers,
  findGotoReferrersForPhase,
} from './processDefinitionUpdater';

// ── 测试夹具 ──────────────────────────────────────

// 构造一个空的 ProcessDefinition
function makeEmptyDefinition(): ProcessDefinition {
  return {
    process: {
      name: 'test',
      display_name: '测试',
    },
  };
}

// 构造一个含两个 phase、各一个 link 的 definition
// phase1 → link1，phase2 → link2
// link1.on_success: link2
function makeDefinitionWithGoto(): ProcessDefinition {
  return {
    process: { name: 'test', display_name: '测试' },
    phases: [
      {
        id: 'phase1',
        name: '阶段1',
        links: [
          {
            id: 'link1',
            name: '环节1',
            on_success: 'link2',
            on_gate_fail: 'break',
          },
        ],
      },
      {
        id: 'phase2',
        name: '阶段2',
        links: [
          {
            id: 'link2',
            name: '环节2',
            on_success: 'next',
            on_gate_fail: 'break',
          },
        ],
      },
    ],
  };
}

// 构造一个 link 用于 addLink 测试
function makeLink(id: string, name: string): LinkDefinition {
  return { id, name, on_success: 'next', on_gate_fail: 'break' };
}

// ── addPhase 测试 ─────────────────────────────────

describe('addPhase', () => {
  it('addPhase_emptyPhases_addsPhase', () => {
    const def = makeEmptyDefinition();
    const newPhase: PhaseDefinition = { id: 'p1', name: '新阶段' };

    const result = addPhase(def, newPhase);

    expect(result.phases).toBeDefined();
    expect(result.phases!.length).toBe(1);
    expect(result.phases![0].id).toBe('p1');
  });

  it('addPhase_doesNotMutateOriginal', () => {
    const def = makeEmptyDefinition();
    const newPhase: PhaseDefinition = { id: 'p1', name: '新阶段' };

    addPhase(def, newPhase);

    // 原 def 不应被修改
    expect(def.phases).toBeUndefined();
  });
});

// ── removePhase 测试 ──────────────────────────────

describe('removePhase', () => {
  it('removePhase_removesPhaseAndResetsGoto', () => {
    // link1.on_success: link2，删除 phase2（含 link2）
    // link1.on_success 应重置为 next
    const def = makeDefinitionWithGoto();

    const result = removePhase(def, 'phase2');

    // phase2 被删除
    expect(result.phases!.length).toBe(1);
    expect(result.phases![0].id).toBe('phase1');
    // link1 的 on_success 从 link2 重置为 next
    expect(result.phases![0].links![0].on_success).toBe('next');
  });

  it('removePhase_doesNotMutateOriginal', () => {
    const def = makeDefinitionWithGoto();

    removePhase(def, 'phase2');

    // 原 def 的 link1.on_success 仍是 link2
    expect(def.phases![0].links![0].on_success).toBe('link2');
  });
});

// ── addLink 测试 ──────────────────────────────────

describe('addLink', () => {
  it('addLink_appendsLinkToPhase', () => {
    const def = makeDefinitionWithGoto();
    const newLink = makeLink('link3', '环节3');

    const result = addLink(def, 'phase1', newLink);

    // phase1 下现在有 2 个 link
    expect(result.phases![0].links!.length).toBe(2);
    expect(result.phases![0].links![1].id).toBe('link3');
  });

  it('addLink_phaseNotFound_returnsOriginal', () => {
    const def = makeDefinitionWithGoto();
    const newLink = makeLink('link3', '环节3');

    // 不存在的 phase id
    const result = addLink(def, 'nonexistent', newLink);

    // 返回原 definition（结构相同）
    expect(result.phases!.length).toBe(def.phases!.length);
  });
});

// ── removeLink 测试 ──────────────────────────────

describe('removeLink', () => {
  it('removeLink_removesLinkAndResetsGoto', () => {
    // link1.on_success: link2，删除 link2
    // link1.on_success 应重置为 next
    const def = makeDefinitionWithGoto();

    const result = removeLink(def, 'link2');

    // phase2 下的 link2 被删除
    expect(result.phases![1].links!.length).toBe(0);
    // link1 的 on_success 从 link2 重置为 next
    expect(result.phases![0].links![0].on_success).toBe('next');
  });

  it('removeLink_doesNotMutateOriginal', () => {
    const def = makeDefinitionWithGoto();

    removeLink(def, 'link2');

    // 原 def 的 phase2 下仍有 link2
    expect(def.phases![1].links!.length).toBe(1);
  });
});

// ── updateLinkField 测试 ──────────────────────────

describe('updateLinkField', () => {
  it('updateLinkField_changesFieldValue', () => {
    const def = makeDefinitionWithGoto();

    // 把 link1 的 name 改为 '新名称'
    const result = updateLinkField(def, 'phase1', 'link1', 'name', '新名称');

    expect(result.phases![0].links![0].name).toBe('新名称');
  });

  it('updateLinkField_doesNotMutateOriginal', () => {
    const def = makeDefinitionWithGoto();

    updateLinkField(def, 'phase1', 'link1', 'name', '新名称');

    expect(def.phases![0].links![0].name).toBe('环节1');
  });
});

// ── updatePhaseField 测试 ─────────────────────────

describe('updatePhaseField', () => {
  it('updatePhaseField_changesFieldValue', () => {
    const def = makeDefinitionWithGoto();

    // 把 phase1 的 name 改为 '新阶段名'
    const result = updatePhaseField(def, 'phase1', 'name', '新阶段名');

    expect(result.phases![0].name).toBe('新阶段名');
  });
});

// ── setLinkGoto 测试 ──────────────────────────────

describe('setLinkGoto', () => {
  it('setLinkGoto_setsOnSuccessGoto', () => {
    const def = makeDefinitionWithGoto();

    // 把 link1 的 on_success 设为 link2（已是该值，验证函数正常工作）
    const result = setLinkGoto(def, 'phase1', 'link1', 'on_success', 'link2');

    expect(result.phases![0].links![0].on_success).toBe('link2');
  });

  it('setLinkGoto_setsOnGateFailGoto', () => {
    const def = makeDefinitionWithGoto();

    // 把 link1 的 on_gate_fail 设为 link2
    const result = setLinkGoto(def, 'phase1', 'link1', 'on_gate_fail', 'link2');

    expect(result.phases![0].links![0].on_gate_fail).toBe('link2');
  });
});

// ── resetLinkGoto 测试 ────────────────────────────

describe('resetLinkGoto', () => {
  it('resetLinkGoto_resetsOnSuccessToNext', () => {
    const def = makeDefinitionWithGoto();
    // link1.on_success 当前是 link2

    const result = resetLinkGoto(def, 'phase1', 'link1', 'on_success');

    // 重置为默认值 'next'
    expect(result.phases![0].links![0].on_success).toBe('next');
  });

  it('resetLinkGoto_resetsOnGateFailToBreak', () => {
    const def = makeDefinitionWithGoto();
    // 先把 link1.on_gate_fail 设为 link2
    const withGoto = setLinkGoto(def, 'phase1', 'link1', 'on_gate_fail', 'link2');

    const result = resetLinkGoto(withGoto, 'phase1', 'link1', 'on_gate_fail');

    // 重置为默认值 'break'
    expect(result.phases![0].links![0].on_gate_fail).toBe('break');
  });
});

// ── findGotoReferrers 测试 ────────────────────────

describe('findGotoReferrers', () => {
  it('findGotoReferrers_findsAllReferrers', () => {
    const def = makeDefinitionWithGoto();
    // link1.on_success: link2，所以 link2 的引用者是 link1

    const referrers = findGotoReferrers(def, 'link2');

    expect(referrers.length).toBe(1);
    expect(referrers[0].linkId).toBe('link1');
    expect(referrers[0].field).toBe('on_success');
  });

  it('findGotoReferrers_noReferrers_returnsEmpty', () => {
    const def = makeDefinitionWithGoto();
    // link1 没有被任何 goto 引用

    const referrers = findGotoReferrers(def, 'link1');

    expect(referrers.length).toBe(0);
  });
});

// ── findGotoReferrersForPhase 测试 ────────────────

describe('findGotoReferrersForPhase', () => {
  it('findGotoReferrersForPhase_findsReferrersForPhaseLinks', () => {
    const def = makeDefinitionWithGoto();
    // phase2 含 link2，link1.on_success: link2
    // 所以 phase2 的引用者是 link1

    const referrers = findGotoReferrersForPhase(def, 'phase2');

    expect(referrers.length).toBe(1);
    expect(referrers[0].linkId).toBe('link1');
  });

  it('findGotoReferrersForPhase_phaseNotFound_returnsEmpty', () => {
    const def = makeDefinitionWithGoto();

    const referrers = findGotoReferrersForPhase(def, 'nonexistent');

    expect(referrers.length).toBe(0);
  });
});
