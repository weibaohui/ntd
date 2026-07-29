// processDefinitionUpdater.ts
// ---------------------------------------------------------------------------
// M4 里程碑：ProcessDefinition 不可变更新纯函数模块。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.6）：
// - 所有更新返回新的 ProcessDefinition 对象，不修改原对象（React 不可变更新）。
// - 级联重置悬空 goto 引用的逻辑集中在这里，单元测试覆盖。
// - 纯函数无副作用，便于 vitest 测试。
//
// 核心规则：
// - 删除 link → 遍历所有 link，把 on_success/on_gate_fail 指向被删 link id 的重置为默认值
//   （需求 037：跳转值是裸环节 id，已清除 goto: 前缀）。
// - 删除 phase → 遍历其下所有 link，对每一个执行上述级联重置。
//
// ID 约定（M4 用数组索引，M5 可改用 phase.id/link.id）：
// - phase 节点 id：`phase-${phaseIndex}`
// - link 节点 id：`link-${phaseIndex}-${linkIndex}`
// 本模块不依赖节点 id，只依赖 phase.id / link.id（YAML 里的真实 id）。
// ---------------------------------------------------------------------------

import type {
  ProcessDefinition,
  PhaseDefinition,
  LinkDefinition,
} from '@/types/process';

// ── 工具函数 ──────────────────────────────────────────

// 深拷贝 definition（结构化克隆，确保不可变更新）。
// 用 JSON 序列化而非 structuredClone，兼容性更好且 definition 是纯数据。
function cloneDefinition(definition: ProcessDefinition): ProcessDefinition {
  return JSON.parse(JSON.stringify(definition)) as ProcessDefinition;
}

// 默认 on_success 值（删除 goto 后回退）
const DEFAULT_ON_SUCCESS = 'next';
// 默认 on_gate_fail 值（删除 goto 后回退）
const DEFAULT_ON_GATE_FAIL = 'break';

// 判断 on_success / on_gate_fail 是否指向指定 link id。
// 需求 037：跳转值用裸环节 id（已清除 goto: 前缀），直接比较即可。
// 保留字 next/end/break/skip 不会等于真实 link id，无需额外排除。
function isGotoTarget(value: string | undefined, targetLinkId: string): boolean {
  return !!value && value === targetLinkId;
}

// ── 查询函数 ──────────────────────────────────────────

// 查找所有引用指定 link 的 goto。
// 返回引用者列表，每个引用者包含 phaseId / linkId / field（on_success | on_gate_fail）。
export function findGotoReferrers(
  definition: ProcessDefinition,
  targetLinkId: string,
): Array<{ phaseId: string; linkId: string; field: 'on_success' | 'on_gate_fail' }> {
  const referrers: Array<{
    phaseId: string;
    linkId: string;
    field: 'on_success' | 'on_gate_fail';
  }> = [];

  // 遍历所有 phase 下的所有 link，检查 on_success / on_gate_fail
  for (const phase of definition.phases ?? []) {
    for (const link of phase.links ?? []) {
      if (isGotoTarget(link.on_success, targetLinkId)) {
        referrers.push({
          phaseId: phase.id,
          linkId: link.id,
          field: 'on_success',
        });
      }
      if (isGotoTarget(link.on_gate_fail, targetLinkId)) {
        referrers.push({
          phaseId: phase.id,
          linkId: link.id,
          field: 'on_gate_fail',
        });
      }
    }
  }

  return referrers;
}

// 查找所有引用指定 phase 下任意 link 的 goto。
// 用于删除 phase 时的级联重置提示。
export function findGotoReferrersForPhase(
  definition: ProcessDefinition,
  targetPhaseId: string,
): Array<{ phaseId: string; linkId: string; field: 'on_success' | 'on_gate_fail' }> {
  // 找到目标 phase
  const targetPhase = (definition.phases ?? []).find(
    (p) => p.id === targetPhaseId,
  );
  if (!targetPhase) return [];

  // 收集目标 phase 下所有 link id
  const targetLinkIds = (targetPhase.links ?? []).map((l) => l.id);

  // 对每个 target link id 调用 findGotoReferrers，合并结果
  const allReferrers: Array<{
    phaseId: string;
    linkId: string;
    field: 'on_success' | 'on_gate_fail';
  }> = [];
  for (const targetLinkId of targetLinkIds) {
    allReferrers.push(...findGotoReferrers(definition, targetLinkId));
  }

  return allReferrers;
}

// ── 更新函数 ──────────────────────────────────────────

// 新增 phase 到 definition 末尾。
export function addPhase(
  definition: ProcessDefinition,
  phase: PhaseDefinition,
): ProcessDefinition {
  const cloned = cloneDefinition(definition);
  // 确保 phases 数组存在
  if (!cloned.phases) cloned.phases = [];
  cloned.phases.push(phase);
  return cloned;
}

// 删除 phase，并级联重置悬空 goto 引用。
// 级联规则：遍历被删 phase 下所有 link id，对所有引用这些 link 的 goto 执行重置。
export function removePhase(
  definition: ProcessDefinition,
  phaseId: string,
): ProcessDefinition {
  const cloned = cloneDefinition(definition);
  if (!cloned.phases) return cloned;

  // 找到被删 phase，收集其下所有 link id
  const removedPhase = cloned.phases.find((p) => p.id === phaseId);
  const removedLinkIds = (removedPhase?.links ?? []).map((l) => l.id);

  // 从 phases 数组移除该 phase
  cloned.phases = cloned.phases.filter((p) => p.id !== phaseId);

  // 级联重置：遍历剩余所有 link，把指向被删 link 的 goto 重置为默认值
  resetGotoForTargets(cloned, removedLinkIds);

  return cloned;
}

// 新增 link 到指定 phase。
export function addLink(
  definition: ProcessDefinition,
  phaseId: string,
  link: LinkDefinition,
): ProcessDefinition {
  const cloned = cloneDefinition(definition);
  if (!cloned.phases) return cloned;

  // 找到目标 phase，在其 links 数组末尾追加
  const phase = cloned.phases.find((p) => p.id === phaseId);
  if (!phase) return cloned;
  if (!phase.links) phase.links = [];
  phase.links.push(link);

  return cloned;
}

// 删除 link，并级联重置悬空 goto 引用。
// 级联规则：对所有引用被删 link 的 goto 执行重置。
export function removeLink(
  definition: ProcessDefinition,
  linkId: string,
): ProcessDefinition {
  const cloned = cloneDefinition(definition);
  if (!cloned.phases) return cloned;

  // 从所有 phase 的 links 数组中移除该 link
  for (const phase of cloned.phases) {
    if (phase.links) {
      phase.links = phase.links.filter((l) => l.id !== linkId);
    }
  }

  // 级联重置：把所有指向被删 link 的 goto 重置为默认值
  resetGotoForTargets(cloned, [linkId]);

  return cloned;
}

// 更新 link 的指定字段。
// field 是 LinkDefinition 的键，value 是新值。
export function updateLinkField(
  definition: ProcessDefinition,
  phaseId: string,
  linkId: string,
  field: keyof LinkDefinition,
  value: unknown,
): ProcessDefinition {
  const cloned = cloneDefinition(definition);
  if (!cloned.phases) return cloned;

  // 找到目标 phase → 目标 link → 更新字段
  const phase = cloned.phases.find((p) => p.id === phaseId);
  if (!phase || !phase.links) return cloned;
  const link = phase.links.find((l) => l.id === linkId);
  if (!link) return cloned;

  // 用 as unknown as Record 类型断言安全地赋值
  // 先转 unknown 再转 Record，避免 TS "insufficient overlap" 错误
  (link as unknown as Record<string, unknown>)[field] = value;

  return cloned;
}

// 更新 phase 的指定字段。
export function updatePhaseField(
  definition: ProcessDefinition,
  phaseId: string,
  field: keyof PhaseDefinition,
  value: unknown,
): ProcessDefinition {
  const cloned = cloneDefinition(definition);
  if (!cloned.phases) return cloned;

  const phase = cloned.phases.find((p) => p.id === phaseId);
  if (!phase) return cloned;

  // 先转 unknown 再转 Record，避免 TS "insufficient overlap" 错误
  (phase as unknown as Record<string, unknown>)[field] = value;

  return cloned;
}

// 拖连线后更新 on_success / on_gate_fail 为跳转目标（裸环节 id，需求 037）。
// handleType 区分是 on_success 还是 on_gate_fail。
export function setLinkGoto(
  definition: ProcessDefinition,
  sourcePhaseId: string,
  sourceLinkId: string,
  handleType: 'on_success' | 'on_gate_fail',
  targetLinkId: string,
): ProcessDefinition {
  // 复用 updateLinkField，把字段值设为目标环节 id（裸，无 goto: 前缀）
  return updateLinkField(
    definition,
    sourcePhaseId,
    sourceLinkId,
    handleType,
    targetLinkId,
  );
}

// 删除连线后重置 on_success / on_gate_fail 为默认值。
export function resetLinkGoto(
  definition: ProcessDefinition,
  sourcePhaseId: string,
  sourceLinkId: string,
  handleType: 'on_success' | 'on_gate_fail',
): ProcessDefinition {
  // 根据字段类型选择默认值
  const defaultValue =
    handleType === 'on_success' ? DEFAULT_ON_SUCCESS : DEFAULT_ON_GATE_FAIL;
  return updateLinkField(
    definition,
    sourcePhaseId,
    sourceLinkId,
    handleType,
    defaultValue,
  );
}

// ── 内部级联重置函数 ──────────────────────────────────

// 对 definition 中所有指向 targetLinkIds 的 goto 执行重置。
// 这是 removeLink / removePhase 的共享级联逻辑。
function resetGotoForTargets(
  definition: ProcessDefinition,
  targetLinkIds: string[],
): void {
  // 转为 Set 加速查找
  const targetSet = new Set(targetLinkIds);
  if (targetSet.size === 0) return;

  // 遍历所有 phase 下的所有 link
  for (const phase of definition.phases ?? []) {
    for (const link of phase.links ?? []) {
      // 检查 on_success 是否指向被删 link（裸环节 id，需求 037）
      if (link.on_success && targetSet.has(link.on_success)) {
        link.on_success = DEFAULT_ON_SUCCESS;
      }
      // 检查 on_gate_fail 是否指向被删 link
      if (link.on_gate_fail && targetSet.has(link.on_gate_fail)) {
        link.on_gate_fail = DEFAULT_ON_GATE_FAIL;
      }
    }
  }
}
