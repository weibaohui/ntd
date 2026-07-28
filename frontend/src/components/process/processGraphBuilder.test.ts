// processGraphBuilder.test.ts
// ---------------------------------------------------------------------------
// M4 里程碑：processGraphBuilder 的 vitest 单元测试。
//
// 覆盖场景：
// 1. buildProcessGraph：null definition 返回空图
// 2. buildProcessGraph：空 phases 返回空图
// 3. buildProcessGraph：单个 phase + link 生成节点
// 4. buildProcessGraph：goto on_success 生成绿色边
// 5. buildProcessGraph：goto on_gate_fail 生成橙色虚线边
// 6. buildProcessGraph：悬空 goto（目标 link 不存在）不生成边
//
// 回调：测试用 mock 回调，验证节点 data 中包含正确回调。
// ---------------------------------------------------------------------------

import { describe, it, expect, vi } from 'vitest';
import type { ProcessDefinition, LinkDefinition } from '@/types/process';
import { buildProcessGraph } from './processGraphBuilder';

// ── 测试夹具 ──────────────────────────────────────

function makeLink(id: string, name: string): LinkDefinition {
  return { id, name, on_success: 'next', on_gate_fail: 'break' };
}

// 单个 phase + 单个 link，无 goto
function makeSimpleDefinition(): ProcessDefinition {
  return {
    process: { name: 'test', display_name: '测试' },
    phases: [
      {
        id: 'phase1',
        name: '阶段1',
        links: [makeLink('link1', '环节1')],
      },
    ],
  };
}

// 两个 phase、各一个 link，link1.on_success: goto:link2
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
            on_success: 'goto:link2',
            on_gate_fail: 'break',
          },
        ],
      },
      {
        id: 'phase2',
        name: '阶段2',
        links: [makeLink('link2', '环节2')],
      },
    ],
  };
}

// 构造 mock 回调
function makeCallbacks() {
  return {
    onDeletePhase: vi.fn(),
    onSelectPhase: vi.fn(),
    onSelectLink: vi.fn(),
    onDeleteLink: vi.fn(),
    onDeleteEdge: vi.fn(),
    onAddLink: vi.fn(),
  };
}

// ── 空图测试 ──────────────────────────────────────

describe('buildProcessGraph', () => {
  it('buildProcessGraph_nullDefinition_returnsEmptyGraph', () => {
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(null, callbacks);

    expect(result.nodes.length).toBe(0);
    expect(result.edges.length).toBe(0);
  });

  it('buildProcessGraph_emptyPhases_returnsEmptyGraph', () => {
    const def: ProcessDefinition = {
      process: { name: 'test', display_name: '测试' },
    };
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);

    expect(result.nodes.length).toBe(0);
    expect(result.edges.length).toBe(0);
  });
});

// ── 节点生成测试 ──────────────────────────────────

describe('buildProcessGraph nodes', () => {
  it('buildProcessGraph_singlePhaseWithLink_generatesNodes', () => {
    const def = makeSimpleDefinition();
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);

    // 1 个 phase 节点 + 1 个 link 节点 = 2 个节点
    expect(result.nodes.length).toBe(2);

    // phase 节点
    const phaseNode = result.nodes.find((n) => n.id === 'phase-0');
    expect(phaseNode).toBeDefined();
    expect(phaseNode!.type).toBe('phase');

    // link 节点
    const linkNode = result.nodes.find((n) => n.id === 'link-0-0');
    expect(linkNode).toBeDefined();
    expect(linkNode!.type).toBe('link');
    // React Flow v12 用 parentId 而非 parentNode
    expect(linkNode!.parentId).toBe('phase-0');
  });

  it('buildProcessGraph_phaseNodeContainsPhaseData', () => {
    const def = makeSimpleDefinition();
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);
    const phaseNode = result.nodes.find((n) => n.id === 'phase-0')!;

    // data.phase 应包含 phase 定义
    // React Flow v12 的 Node.data 是 unknown，用 as 断言提取
    const phaseData = phaseNode.data as {
      phase: { id: string; name: string };
      phaseIndex: number;
      onDeletePhase: typeof callbacks.onDeletePhase;
      onSelectPhase: typeof callbacks.onSelectPhase;
    };
    expect(phaseData.phase).toBeDefined();
    expect(phaseData.phase.id).toBe('phase1');
    expect(phaseData.phase.name).toBe('阶段1');
    // data.phaseIndex 应为 0
    expect(phaseData.phaseIndex).toBe(0);
    // data 应包含回调
    expect(phaseData.onDeletePhase).toBe(callbacks.onDeletePhase);
    expect(phaseData.onSelectPhase).toBe(callbacks.onSelectPhase);
  });

  it('buildProcessGraph_linkNodeContainsLinkData', () => {
    const def = makeSimpleDefinition();
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);
    const linkNode = result.nodes.find((n) => n.id === 'link-0-0')!;

    // data.link 应包含 link 定义
    // React Flow v12 的 Node.data 是 unknown，用 as 断言提取
    const linkData = linkNode.data as {
      link: { id: string; name: string };
      phaseId: string;
      onSelectLink: typeof callbacks.onSelectLink;
    };
    expect(linkData.link).toBeDefined();
    expect(linkData.link.id).toBe('link1');
    expect(linkData.link.name).toBe('环节1');
    // data.phaseId 应为 phase1
    expect(linkData.phaseId).toBe('phase1');
    // data 应包含回调
    expect(linkData.onSelectLink).toBe(callbacks.onSelectLink);
  });
});

// ── 边生成测试 ────────────────────────────────────

describe('buildProcessGraph edges', () => {
  it('buildProcessGraph_gotoOnSuccess_generatesGreenEdge', () => {
    const def = makeDefinitionWithGoto();
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);

    // 应生成 goto 成功边（另有阶段间灰线，故用 sourceHandle 定位）
    const edge = result.edges.find((e) => e.sourceHandle === 'on_success')!;
    expect(edge).toBeDefined();

    // 边类型为 process（自定义边）
    expect(edge.type).toBe('process');
    // sourceHandle = on_success
    expect(edge.sourceHandle).toBe('on_success');
    // targetHandle = target
    expect(edge.targetHandle).toBe('target');
    // 颜色为绿色 #10b981
    expect(edge.data?.color).toBe('#10b981');
    // on_success goto 不虚线
    expect(edge.data?.dashed).toBe(false);
  });

  it('buildProcessGraph_gotoOnGateFail_generatesOrangeDashedEdge', () => {
    // 构造 link1.on_gate_fail: goto:link2
    const def: ProcessDefinition = {
      process: { name: 'test', display_name: '测试' },
      phases: [
        {
          id: 'phase1',
          name: '阶段1',
          links: [
            {
              id: 'link1',
              name: '环节1',
              on_success: 'next',
              on_gate_fail: 'goto:link2',
            },
          ],
        },
        {
          id: 'phase2',
          name: '阶段2',
          links: [makeLink('link2', '环节2')],
        },
      ],
    };
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);

    // 阶段间灰线也计入 edges，用 sourceHandle 精确定位 goto 边
    const edge = result.edges.find((e) => e.sourceHandle === 'on_gate_fail')!;
    expect(edge).toBeDefined();

    // sourceHandle = on_gate_fail
    expect(edge.sourceHandle).toBe('on_gate_fail');
    // 颜色为橙色 #d97706
    expect(edge.data?.color).toBe('#d97706');
    // on_gate_fail goto 虚线
    expect(edge.data?.dashed).toBe(true);
  });

  it('buildProcessGraph_danglingGoto_doesNotGenerateEdge', () => {
    // link1.on_success: goto:nonexistent（目标不存在）
    const def: ProcessDefinition = {
      process: { name: 'test', display_name: '测试' },
      phases: [
        {
          id: 'phase1',
          name: '阶段1',
          links: [
            {
              id: 'link1',
              name: '环节1',
              on_success: 'goto:nonexistent',
              on_gate_fail: 'break',
            },
          ],
        },
      ],
    };
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);

    // 悬空 goto 不生成边
    expect(result.edges.length).toBe(0);
  });

  it('buildProcessGraph_edgeDataContainsDeleteCallback', () => {
    const def = makeDefinitionWithGoto();
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);
    const edge = result.edges[0];

    // data.onDelete 应为 callbacks.onDeleteEdge
    expect(edge.data?.onDelete).toBe(callbacks.onDeleteEdge);
  });

  it('buildProcessGraph_generatesPhaseFlowEdgesBetweenAdjacentPhases', () => {
    // 3 个 phase（无 goto）→ 应生成 2 条阶段间顺向灰线
    const def: ProcessDefinition = {
      process: { name: 'test', display_name: '测试' },
      phases: [
        { id: 'phase1', name: '阶段1', links: [makeLink('link1', '环节1')] },
        { id: 'phase2', name: '阶段2', links: [makeLink('link2', '环节2')] },
        { id: 'phase3', name: '阶段3', links: [makeLink('link3', '环节3')] },
      ],
    };
    const callbacks = makeCallbacks();

    const result = buildProcessGraph(def, callbacks);

    // 阶段边 id 以 edge-phase- 开头，区别于 goto 边
    const phaseEdges = result.edges.filter((e) =>
      e.id.startsWith('edge-phase-'),
    );
    expect(phaseEdges.length).toBe(2);
    expect(phaseEdges[0].source).toBe('phase-0');
    expect(phaseEdges[0].target).toBe('phase-1');
    expect(phaseEdges[1].source).toBe('phase-1');
    expect(phaseEdges[1].target).toBe('phase-2');
    // 灰色实线，不可删（无 onDelete）
    expect(phaseEdges[0].data?.color).toBe('#94a3b8');
    expect(phaseEdges[0].data?.dashed).toBe(false);
    expect(phaseEdges[0].data?.onDelete).toBeUndefined();
  });
});
