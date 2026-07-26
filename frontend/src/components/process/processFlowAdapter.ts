// 工艺 YAML 定义 → 流程图数据适配器。
// 把 ProcessDefinition（来自 js-yaml 解析）转换为 useFlowLayout 所需的节点/边输入，
// 供 ProcessFlowGraph 直接调用。
//
// 设计约束：
// - 链接的 on_success 字段（next / goto:<id> / end）是唯一边判据。
// - on_gate_fail 导致的 goto 也作为边（虚线），与成功边区分但共用 dagre 布局。
// - 阶段边界通过 PhaseGroup + ProcessFlowGraph 内的 PhaseHeader 标签呈现（不改 dagre 布局）。

import type { LinkDefinition, ProcessDefinition } from '@/types/process';
import type { FlowNodeInput, FlowEdgeInput } from '@/components/loop-flow/useFlowLayout';
import { NODE_WIDTH, NODE_HEIGHT, START_NODE_ID, END_NODE_ID } from '@/components/loop-flow/flowConstants';
import yaml from 'js-yaml';

/** 适配器输出的一条链接信息（含阶段上下文、生成了数字 ID）。 */
export interface AdaptedLink {
  /** 链接在全局扁平列表中的序号（0-based），也是节点 id。 */
  numericId: number;
  /** YAML 中的原始 id（如 "calc"、"review"）。 */
  stringId: string;
  /** 链接名称 */
  name: string;
  /** 所属阶段 id */
  phaseId: string;
  /** 所属阶段名称 */
  phaseName: string;
  /** 完整链路定义 */
  link: LinkDefinition;
}

/** 阶段分组信息（供 ProcessFlowGraph 绘制阶段标签）。 */
export interface PhaseGroup {
  phaseId: string;
  phaseName: string;
  /** links 中的起始下标（含） */
  startIndex: number;
  /** links 中的结束下标（不含） */
  endIndex: number;
}

export interface AdaptedFlow {
  /** 摊平后的链接列表（含数字 id）。 */
  links: AdaptedLink[];
  /** 用于 dagre 布局的节点输入。 */
  nodeInputs: FlowNodeInput[];
  /** 用于 dagre 布局的边输入。 */
  edgeInputs: FlowEdgeInput[];
  /** 布局用边（含标签，供 ProcessFlowGraph 画边时标注 on_success / on_gate_fail）。 */
  templateEdges: TemplateEdge[];
  /** 阶段分组（供 ProcessFlowGraph 渲染阶段标签）。 */
  phaseGroups: PhaseGroup[];
  /** 全局限制 */
  limits?: { max_step_executions?: number; max_total_tokens?: number };
}

/** 一条流程边（从 template 语义抽象，比 LayoutEdge 更简单）。 */
export interface TemplateEdge {
  fromNumericId: number;
  toNumericId: number;
  label: string;
  kind: 'success' | 'fail-goto' | 'end';
}

/**
 * 从工艺模板的定义 YAML 文本构建适配数据。
 *
 * 解析失败（非法的 YAML 或缺少 process key）返回 null，调用方自行提示。
 */
export function adaptProcessDefinition(yamlText: string): AdaptedFlow | null {
  let def: ProcessDefinition;
  try {
    def = yaml.load(yamlText) as ProcessDefinition;
  } catch {
    return null;
  }
  if (!def || !def.process) return null;

  const links: AdaptedLink[] = [];
  // 扁平化：按阶段顺序摊平 links
  for (const phase of def.phases ?? []) {
    for (const link of phase.links ?? []) {
      links.push({
        numericId: links.length,
        stringId: link.id,
        name: link.name || link.id,
        phaseId: phase.id,
        phaseName: phase.name || phase.id,
        link,
      });
    }
  }

  // 构建 id 映射（stringId → numericId）
  const idMap = new Map<string, number>();
  for (const l of links) {
    idMap.set(l.stringId, l.numericId);
  }

  // 节点输入
  const nodeInputs: FlowNodeInput[] = links.map(l => ({
    id: l.numericId,
    width: NODE_WIDTH,
    height: NODE_HEIGHT,
  }));

  // 边输入
  const edgeInputs: FlowEdgeInput[] = [];
  const templateEdges: TemplateEdge[] = [];

  // Start → 第一个链接
  if (links.length > 0) {
    edgeInputs.push({ from: START_NODE_ID, to: links[0].numericId, label: '' });
    templateEdges.push({ fromNumericId: START_NODE_ID, toNumericId: links[0].numericId, label: '', kind: 'success' });
  }

  for (let i = 0; i < links.length; i++) {
    const link = links[i];
    const onSuccess = link.link.on_success || 'next';
    const onGateFail = link.link.on_gate_fail || 'break';

    // 成功边
    const successTarget = resolveTransitionTarget(onSuccess, i, links, idMap);
    if (successTarget !== null) {
      const isForward = successTarget === END_NODE_ID || successTarget === i + 1;
      // 只有顺向边（next / end）才加入 dagre，goto 反向边会导致多行 layout
      if (isForward) {
        edgeInputs.push({ from: link.numericId, to: successTarget, label: '' });
      }
      templateEdges.push({
        fromNumericId: link.numericId, toNumericId: successTarget,
        label: onSuccess === 'next' ? '' : onSuccess.startsWith('goto:') ? `→${onSuccess.slice(5)}` : '',
        kind: 'success',
      });
    }

    // 门禁失败边（仅当不同于成功策略）
    if (onGateFail !== onSuccess && onGateFail !== 'break') {
      const failTarget = resolveTransitionTarget(onGateFail, i, links, idMap);
      if (failTarget !== null && failTarget !== successTarget) {
        const isForward = failTarget === END_NODE_ID || failTarget === i + 1;
        if (isForward) {
          edgeInputs.push({ from: link.numericId, to: failTarget, label: '' });
        }
        templateEdges.push({
          fromNumericId: link.numericId, toNumericId: failTarget,
          label: `门禁失败 ${onGateFail.startsWith('goto:') ? `→${onGateFail.slice(5)}` : ''}`,
          kind: 'fail-goto',
        });
      }
    }
  }

  // 构建阶段分组（供 ProcessFlowGraph 绘制阶段标签）
  const phaseGroups: PhaseGroup[] = [];
  let groupStart = 0;
  for (let i = 1; i <= links.length; i++) {
    // 阶段变更或到达末尾 → 关闭当前分组
    if (i === links.length || links[i].phaseId !== links[groupStart].phaseId) {
      phaseGroups.push({
        phaseId: links[groupStart].phaseId,
        phaseName: links[groupStart].phaseName,
        startIndex: groupStart,
        endIndex: i,
      });
      groupStart = i;
    }
  }

  return {
    links,
    nodeInputs,
    edgeInputs,
    templateEdges,
    phaseGroups,
    limits: def.limits,
  };
}

/**
 * 解析 on_success / on_gate_fail 策略，返回目标链接的 numericId。
 * - "next"  → 当前 phase 内下一个链接（若已是最后一个则 → END）
 * - "end"   → END_NODE_ID
 * - "goto:<id>" → 对应 stringId 的 numericId，若未找到 → null（容错）
 * - "break"  → null（不连线）
 * - 其他     → null
 */
function resolveTransitionTarget(
  strategy: string,
  currentIndex: number,
  links: AdaptedLink[],
  idMap: Map<string, number>,
): number | null {
  const s = strategy.trim();
  if (s === 'next') {
    // 在当前 phase 内找下一个链接（phase 内顺序即全局摊平后的连续段）。
    // 简化：直接找全局下一个，不区分 phase 边界（goto 跨 phase 也允许）。
    if (currentIndex + 1 < links.length) {
      return links[currentIndex + 1].numericId;
    }
    return END_NODE_ID;
  }
  if (s === 'end') {
    return END_NODE_ID;
  }
  if (s.startsWith('goto:')) {
    const targetStr = s.slice(5).trim();
    // goto 目标可能是 "calc" 也可能是 "goto:calc" 两次嵌套？截取第一层。
    const id = idMap.get(targetStr);
    return id != null ? id : null;
  }
  // break / skip / 未知 → 不画边
  return null;
}
