// 概念导航首页静态数据。
// 6 个核心概念 + 关系图节点。
// 集中管理避免 ConceptRelationGraph/ConceptCardGrid/ConceptDetailSection 各写一套。

import type { ReactNode } from 'react';
import {
  BuildOutlined,
  MacCommandOutlined,
  RetweetOutlined,
  RocketOutlined,
  TeamOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons';
// 030：GraphNode 跳转按钮需要 View 类型（支线节点跳转目标视图）。
import type { View } from '@/hooks/useViewState';

/**
 * 单个概念的定义。
 * 用于关系图 Drawer、卡片网格、详细说明区三处共享。
 */
export interface ConceptNode {
  /** 唯一标识，与关系图节点 id 对齐。 */
  id: 'process' | 'loop' | 'todo' | 'task' | 'executor' | 'expert';
  /** 中文标签，用于卡片标题、Drawer 标题、详细说明区标题。 */
  label: string;
  /** 一句话定义，用于卡片副标题、Drawer 头部。 */
  oneLiner: string;
  /** 概念图标，用于卡片左侧。 */
  icon: ReactNode;
  /** 关键字段表，用于 Drawer + 详细说明区左栏 Descriptions。 */
  fields: ReadonlyArray<{ name: string; desc: string }>;
  /** 跳转目标视图，用于 Drawer + 详细说明区底部的「去 XX 页」按钮。 */
  navTarget: View;
  /** YAML 片段示例，用于详细说明区右栏数据快照空态时展示。 */
  yamlExample: string;
}

/**
 * 6 个核心概念定义。
 *
 * 顺序即卡片网格 + 详细说明区的展示顺序：
 *   工艺 → 环路 → 事项 → 任务 → 执行器 → 专家
 * 这个顺序也是概念从抽象到实例的层级链。
 */
export const CONCEPTS: readonly ConceptNode[] = [
  {
    id: 'process',
    label: '工艺',
    oneLiner: '一套「怎么做」的说明书，规定分几步、每步做什么、做完怎么验收',
    icon: <BuildOutlined />,
    navTarget: 'processes',
    fields: [
      { name: '名字', desc: '这套工艺叫什么，方便日后找' },
      { name: '复杂度', desc: '轻量 / 标准 / 复杂，影响推荐用哪个 AI' },
      { name: '版本号', desc: '工艺改了就升一下，环路装的是哪个版本能追到' },
      { name: '阶段', desc: '分几个大步骤，比如「需求 → 设计 → 编码 → 验收」' },
      { name: '环节', desc: '每个阶段里具体要干的小活，比如「写需求文档」' },
      { name: '门禁', desc: '每步做完怎么检查：看产物 / AI 评 / 人工批 / 脚本查' },
      { name: '产物', desc: '每步做完要交出什么文件，比如「需求.md」' },
    ],
    yamlExample: `# 这就是一套工艺的样子
名字: 4 阶段 12 环节交付
复杂度: 标准
版本: 1.0.0
# 下面是要分几步、每步干啥、做完怎么验收
阶段:
  - 名字: 需求
    环节:
      - 干啥: 找用户访谈，写需求文档
        验收: 人工批（产品经理签字）
  - 名字: 设计
    环节: [...]  # 后面阶段同上格式`,
  },
  {
    id: 'loop',
    label: '环路',
    oneLiner: '把工艺装到电脑里跑起来的流水线，可以反复执行、自动循环',
    icon: <RetweetOutlined />,
    navTarget: 'loops',
    fields: [
      { name: '来源工艺', desc: '这个环路是从哪套工艺装出来的' },
      { name: '运行状态', desc: '启用 / 暂停，控制能不能被任务调起执行' },
      { name: '阶段', desc: '工艺里的大步骤在这里变成可执行的具体节点' },
      { name: '环节', desc: '每个节点要干的事，都挂着一个「事项」当干活指令' },
      { name: '限流', desc: '最多跑几次、最多烧多少 token，防 AI 失控狂跑' },
    ],
    // 044：触发器已整体下线，环路只由「创建任务」唯一入口驱动执行。
    yamlExample: `# 一个环路长这样
名字: 4 阶段 12 环节交付实例
从哪套工艺装的: 4 阶段 12 环节交付 (版本 1.0.0)
状态: 启用
# 怎么把它跑起来：在「任务」页创建一个任务，选这条环路即可执行一次
# 防它失控狂跑的保险
限流:
  最多跑几次: 20
  最多烧多少 token: 100000`,
  },
  {
    id: 'todo',
    label: '事项',
    oneLiner: '环路里每一步要干的活，指定用哪个工具、听谁的指挥、做完算不算合格',
    icon: <UnorderedListOutlined />,
    // 028-命名空间迁移：原 'items' 已统一为 'todos'，与 useViewState 的 View 联合类型一致；
    // 跳转时走 showView('todos') 落到 /#/todos 列表，不再用旧 items 通道。
    navTarget: 'todos',
    fields: [
      { name: '类型', desc: '一次性活 / 可复用环节，决定跑完就扔还是能反复用' },
      { name: '执行器', desc: '用哪把工具干活：Claude Code / MobileCoder ...' },
      { name: '专家', desc: '套哪个人设：产品经理 / 架构师 / 评师 ...' },
      { name: '模型', desc: '具体跑哪个 LLM，不填就用执行器的默认' },
      { name: '指令', desc: '给 AI 的干活命令文本，比如「访谈用户，写需求文档」' },
      { name: '技能', desc: '绑定的 skill 列表，给 AI 加的能力包' },
      { name: '验收标准', desc: '做完怎么算合格，文字描述' },
    ],
    yamlExample: `# 一条事项就是给 AI 的一份干活指令
编号: 42
标题: 收集需求
类型: 可复用环节     # 跑完不扔，能反复用
用哪把工具: Claude Code
套哪个人设: 产品经理
跑哪个模型: claude-sonnet-5    # 不填就用工具的默认
指令: 找用户访谈，写一份需求文档
技能: [需求收集, 用户故事]
验收标准: 需求文档含用户故事 + 验收条件`,
  },
  {
    id: 'task',
    label: '任务',
    oneLiner: '你想让 AI 干的一件事，挑一个环路去执行一次',
    icon: <RocketOutlined />,
    navTarget: 'tasks',
    fields: [
      { name: '标题', desc: '任务叫什么，从你写的需求首行截出来' },
      { name: '需求', desc: '完整描述你想让 AI 干什么' },
      { name: '环路', desc: '挑哪个环路去执行这一次' },
      { name: '状态', desc: '待执行 / 进行中 / 已完成 / 失败' },
      { name: '工艺', desc: '记一下这任务是用哪套工艺跑的，方便事后追' },
    ],
    yamlExample: `# 一个任务就是你想让 AI 干的一件事
编号: 8
标题: 给后端加一个新 API
需求: 需要给后端加一个 /api/v1/tasks 的 POST 接口...
挑哪个环路去跑: 4 阶段 12 环节交付实例
状态: 已完成
用的是哪套工艺: 4 阶段 12 环节交付`,
  },
  {
    id: 'executor',
    label: '执行器',
    oneLiner: '真正干活的那把工具，比如 Claude Code、MobileCoder 这些命令行程序',
    icon: <MacCommandOutlined />,
    navTarget: 'settings',
    fields: [
      { name: '名字', desc: '这把工具叫什么，比如 claudecode' },
      { name: '路径', desc: '命令行程序装在电脑哪个位置' },
      { name: '默认模型', desc: '不特意指定时，这把工具默认跑哪个 LLM' },
    ],
    yamlExample: `# 一把执行器就是一个命令行工具
编号: 1
名字: claudecode
显示名: Claude Code
程序装在哪: /usr/local/bin/claude
默认跑哪个模型: claude-sonnet-5
是不是系统首选: 是`,
  },
  {
    id: 'expert',
    label: '专家',
    oneLiner: '给 AI 套上一层「人设」，让它表现得像产品经理、架构师等某一类角色',
    icon: <TeamOutlined />,
    navTarget: 'settings',
    fields: [
      { name: '名字', desc: '这个人设叫什么，比如 product-manager' },
      { name: '描述', desc: '一句话说这位专家擅长什么' },
      { name: '技能', desc: '绑定的 skill 列表，给 AI 加的能力包' },
      { name: '人设档案', desc: 'Agent MD 写的「你是一位资深产品经理...」指令' },
    ],
    yamlExample: `# 一位专家就是给 AI 套的人设 + 能力包
名字: 产品经理
描述: 擅长需求分析、写用户故事
技能:
  - 需求收集
  - 用户故事
人设档案: |
  你是一位资深产品经理，擅长跟用户访谈、
  挖掘真实需求，输出清晰的用户故事和验收条件...`,
  },
] as const;

/**
 * 关系图节点（SVG 告标）。
 *
 * 坐标系 viewBox 1000x400，手动布局：
 *   - 主链（工艺→环路→事项→执行记录）走横向中线 y=200
 *   - 支线（模板库/执行器专家模型）走上下两侧
 *
 * 044：触发器节点已随触发能力下线移除。
 */
export interface GraphNode {
  id: string;
  label: string;
  x: number;
  y: number;
  /** hover 时同时高亮的节点 id 列表（关联节点）。 */
  highlights: string[];
  /** 对应 CONCEPTS 的 id，用于点击弹概念详情 Drawer；undefined 表示支线节点，弹 fallback 说明 Drawer。 */
  conceptId?: ConceptNode['id'];
  /** 是否主航线节点：true 时圆圈加大 + 主色填充，突出主链层级。 */
  isMain?: boolean;
  // —— 030 新增 3 个可选字段：给「有独立页面」的支线节点（黑板/看板）挂 Drawer 说明 + 跳转按钮。
  // 为什么做成可选字段而非新节点类型：既有 4 个 fallback 节点（触发器/技能/模型/执行记录）不填
  // 这三字段即保持原行为，GraphNode 结构向后兼容，未来新支线节点加跳转也只改数据不动组件。
  /** Drawer 说明文案（仅无 conceptId 的支线节点生效）；缺省回退到通用 fallback 文案。 */
  drawerDesc?: string;
  /** Drawer 底部「去 XX 页」按钮的跳转目标视图；缺省则不渲染跳转按钮。
      用 View 字面量而非手写 hash 字符串：编译期白名单约束，杜绝拼错路由。 */
  navTarget?: View;
}

export const GRAPH_NODES: readonly GraphNode[] = [
  // 主链 4 节点（横向中线，isMain=true 圆圈加大突出主航线）
  // 030：主链节点的 highlights 追加 blackboard/ops —— 高亮是单向声明，
  // 主链侧不声明的话 hover 主链节点时观察层两个新节点不会亮（需求场景 C）。
  { id: 'process', label: '工艺', x: 120, y: 200, highlights: ['loop', 'todo'], conceptId: 'process', isMain: true },
  { id: 'loop', label: '环路', x: 400, y: 200, highlights: ['process', 'todo', 'task', 'blackboard'], conceptId: 'loop', isMain: true },
  { id: 'todo', label: '事项', x: 680, y: 200, highlights: ['loop', 'execution', 'executor', 'expert', 'model', 'skill', 'blackboard'], conceptId: 'todo', isMain: true },
  { id: 'execution', label: '执行记录', x: 900, y: 200, highlights: ['todo', 'blackboard', 'ops'], isMain: true },
  // 支线节点：与 6 核心概念 + skill 对齐，isMain 缺省 false 圆圈较小
  { id: 'task', label: '任务', x: 400, y: 340, highlights: ['loop'], conceptId: 'task' },
  { id: 'executor', label: '执行器', x: 680, y: 60, highlights: ['todo', 'expert', 'model'], conceptId: 'executor' },
  { id: 'expert', label: '专家', x: 760, y: 340, highlights: ['todo', 'executor', 'model'], conceptId: 'expert' },
  { id: 'skill', label: '技能 Skill', x: 580, y: 340, highlights: ['todo', 'expert'] },
  { id: 'model', label: '模型', x: 860, y: 340, highlights: ['todo', 'executor', 'expert'] },
  // 030 观察层 2 节点（支线小圆）：黑板/运行中心是「定义→执行」之后的观察出口。
  // 黑板放执行记录正上方同列（x=900），让「执行记录→黑板」成垂直短边；
  // 运行中心放底行最右端（x=960），与模型圆心距 100（=专家↔模型既有间距），右缘 996 不出 viewBox。
  {
    id: 'blackboard',
    label: '黑板',
    // 与执行记录同列（x=900）：「执行记录→黑板」成垂直短边，避免再增加跨图长斜线
    x: 900,
    // 顶行 y=60 与触发器/执行器同行，维持支线节点上下两行的网格节奏
    y: 60,
    // 高亮单向声明：黑板侧列出三个分析来源，hover 黑板时事项/执行记录/环路同步亮
    highlights: ['todo', 'execution', 'loop'],
    // 三层语义（需求 §5.3）：① 事项+执行记录持续自动分析 ② 环节小黑板记各环节结论 ③ 汇总统一观看。
    drawerDesc:
      '根据事项与执行记录持续自动分析得出的观察报告。环路里还有环节小黑板，逐个记录各个环节的执行结论，汇总到黑板统一观看。',
    navTarget: 'blackboard',
  },
  {
    id: 'ops',
    label: '运行中心',
    // 底行最右端（x=960）：与模型圆心距 100（=专家↔模型既有间距），右缘 996 不出 viewBox
    x: 960,
    // 底行 y=340 与任务/技能/专家/模型同行
    y: 340,
    // 运行中心只有执行记录一个数据来源，故高亮列表只声明它
    highlights: ['execution'],
    drawerDesc: '运行中心聚合运行监控、环路执行历史与完成结论，是「定义→执行」之后的观察出口。',
    // 默认进入运行视图（运行监控为高频核心场景）；原 kanban 进度看板已归位事项菜单，无需 mode 深链。
    navTarget: 'ops',
  },
] as const;

/**
 * 关系图连线。
 * from/to 为 GRAPH_NODES 的 id；label 为连线上的动作语义。
 * isMain=true 表示主航线边：渲染带箭头方向 + 加粗深色，突出主干。
 */
export interface GraphEdge {
  from: string;
  to: string;
  label: string;
  /** 是否主航线：true 时带箭头 + 加粗深色，与支线区分。 */
  isMain?: boolean;
}

export const GRAPH_EDGES: readonly GraphEdge[] = [
  // 主航线 3 条边（工艺→环路→事项→执行记录），带箭头 + 加粗
  { from: 'process', to: 'loop', label: '安装实例化', isMain: true },
  { from: 'loop', to: 'todo', label: '编排引用', isMain: true },
  { from: 'todo', to: 'execution', label: '触发执行', isMain: true },
  // 支线（带箭头方向，细线浅色与主航线区分）
  // 任务：用户主动意图，选环路跑一次（044：环路唯一执行入口）
  { from: 'task', to: 'loop', label: '选择执行' },
  // 执行器/专家/模型：事项的运行时三要素
  { from: 'executor', to: 'todo', label: '运行时' },
  { from: 'expert', to: 'todo', label: '人格' },
  { from: 'skill', to: 'todo', label: '能力注入' },
  { from: 'model', to: 'todo', label: 'LLM' },
  // 030 观察层 4 条边（支线细线，均不带 isMain）：
  // 黑板 = 事项/执行记录持续自动分析 + 环路各环节结论汇总；运行中心 = 运行监控与结论观察。
  // 两条「持续分析」label 相同但 from-to key 不同（todo-/execution-blackboard），React key 无冲突。
  { from: 'todo', to: 'blackboard', label: '持续分析' },
  { from: 'execution', to: 'blackboard', label: '持续分析' },
  { from: 'loop', to: 'blackboard', label: '环节结论' },
  { from: 'execution', to: 'ops', label: '运行监控' },
] as const;

/** Hero 区一句话简介。 */
export const ONBOARDING_HERO_TITLE = 'NTD 是 AI 驱动的任务引擎';
export const ONBOARDING_HERO_SUBTITLE = '用「工艺 → 环路 → 事项」三层抽象，把你的工作自动化';

/** 三个易混概念的对比表（执行器 vs 专家 vs 模型），用于详细说明区。 */
export const EXECUTOR_VS_EXPERT_VS_MODEL: Array<{
  concept: string;
  essence: string;
  example: string;
}> = [
  { concept: '执行器', essence: '运行时 CLI 工具', example: 'claudecode / mobilecoder / opencode' },
  { concept: '专家', essence: '人格 + Skills 组合', example: 'product-manager / architect / code-reviewer' },
  { concept: '模型', essence: '具体跑哪个 LLM', example: 'glm-5.2 / claude-sonnet-5 / gpt-4o' },
] as const;

/** 门禁类型 + 中文标签，用于环路/事项详细说明区。046 起仅 2 类（artifact_present/script_check 已废弃）。 */
export const GATE_TYPES: ReadonlyArray<{ type: string; label: string }> = [
  { type: 'ai_criteria_review', label: 'AI 评审' },
  { type: 'human_approval', label: '人工审批' },
] as const;

/** ThunderboltOutlined 给任务卡用（避免在 ConceptNode 里重复 import）。 */
export { ThunderboltOutlined } from '@ant-design/icons';
