// 概念导航首页静态数据。
// 6 个核心概念 + 关系图节点 + 快速开始 5 步。
// 集中管理避免 ConceptRelationGraph/ConceptCardGrid/ConceptDetailSection/QuickStartFlow 各写一套。

import type { ReactNode } from 'react';
import {
  AppstoreOutlined,
  BuildOutlined,
  CompassOutlined,
  ForwardOutlined,
  MacCommandOutlined,
  RetweetOutlined,
  RocketOutlined,
  TeamOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons';
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
      { name: '运行状态', desc: '启用 / 暂停，控制能不能被触发器调起' },
      { name: '阶段', desc: '工艺里的大步骤在这里变成可执行的具体节点' },
      { name: '环节', desc: '每个节点要干的事，都挂着一个「事项」当干活指令' },
      { name: '触发器', desc: '8 种开关：手动 / 定时 / 钩子 / 飞书 / 事项驱动 ...' },
      { name: '限流', desc: '最多跑几次、最多烧多少 token，防 AI 失控狂跑' },
    ],
    yamlExample: `# 一个环路长这样
名字: 4 阶段 12 环节交付实例
从哪套工艺装的: 4 阶段 12 环节交付 (版本 1.0.0)
状态: 启用
# 下面是什么开关能把它跑起来
触发器:
  - 类型: 手动          # 你点一下才跑
  - 类型: 钚子          # 别人 POST 这个 URL 就跑
  - 类型: 飞书命令      # 在飞书群里发 /ntd run 就跑
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
    navTarget: 'items',
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
 * 呚标系 viewBox 1000x400，手动布局：
 *   - 主链（工艺→环路→事项→执行记录）走横向中线 y=200
 *   - 支线（模板库/触发器/执行器专家模型）走上下两侧
 */
export interface GraphNode {
  id: string;
  label: string;
  x: number;
  y: number;
  /** hover 时同时高亮的节点 id 列表（关联节点）。 */
  highlights: string[];
  /** 对应 CONCEPTS 的 id，用于点击弹 Drawer；undefined 表示支线节点不弹 Drawer。 */
  conceptId?: ConceptNode['id'];
  /** 是否主航线节点：true 时圆圈加大 + 主色填充，突出主链层级。 */
  isMain?: boolean;
}

export const GRAPH_NODES: readonly GraphNode[] = [
  // 主链 4 节点（横向中线，isMain=true 圆圈加大突出主航线）
  { id: 'process', label: '工艺', x: 120, y: 200, highlights: ['loop', 'todo'], conceptId: 'process', isMain: true },
  { id: 'loop', label: '环路', x: 400, y: 200, highlights: ['process', 'todo', 'task', 'trigger'], conceptId: 'loop', isMain: true },
  { id: 'todo', label: '事项', x: 680, y: 200, highlights: ['loop', 'execution', 'trigger', 'executor', 'expert', 'model', 'skill'], conceptId: 'todo', isMain: true },
  { id: 'execution', label: '执行记录', x: 900, y: 200, highlights: ['todo'], isMain: true },
  // 支线节点：与 6 核心概念 + 触发器 + skill 对齐，isMain 缺省 false 圆圈较小
  { id: 'task', label: '任务', x: 400, y: 340, highlights: ['loop'], conceptId: 'task' },
  { id: 'trigger', label: '触发器', x: 440, y: 60, highlights: ['loop', 'todo'] },
  { id: 'executor', label: '执行器', x: 680, y: 60, highlights: ['todo', 'expert', 'model'], conceptId: 'executor' },
  { id: 'expert', label: '专家', x: 760, y: 340, highlights: ['todo', 'executor', 'model'], conceptId: 'expert' },
  { id: 'skill', label: '技能 Skill', x: 580, y: 340, highlights: ['todo', 'expert'] },
  { id: 'model', label: '模型', x: 860, y: 340, highlights: ['todo', 'executor', 'expert'] },
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
  // 任务：用户主动意图，选环路跑一次
  { from: 'task', to: 'loop', label: '选择执行' },
  // 触发器：同时驱动环路和事项（8 种类型里 todo_completed/todo_state_changed/tag_added 驱动事项）
  { from: 'trigger', to: 'loop', label: '驱动环路' },
  { from: 'trigger', to: 'todo', label: '驱动事项' },
  // 执行器/专家/模型：事项的运行时三要素
  { from: 'executor', to: 'todo', label: '运行时' },
  { from: 'expert', to: 'todo', label: '人格' },
  { from: 'skill', to: 'todo', label: '能力注入' },
  { from: 'model', to: 'todo', label: 'LLM' },
] as const;

/**
 * 快速开始 5 步定义。
 *
 * checkApi 字段决定步骤完成判断拉哪个 API：
 *   processes → bundledApi.getProcesses() 非空
 *   triggers  → dbLoops.listLoops() 后查任一 loop 的非 manual 触发器
 *   tasks     → bundledApi.listTasks() 非空
 *   executions → db.getExecutionRecords() 非空
 *   artifacts → 通过 loop_executions 的产物判断（简化：任一 loop 有产物）
 */
export interface QuickStartStep {
  /** 序号，1-5。 */
  index: number;
  /** 步骤标题。 */
  title: string;
  /** 跳转目标视图。 */
  navTarget: View;
  /** 完成判断数据源。 */
  checkApi: 'processes' | 'triggers' | 'tasks' | 'executions' | 'artifacts';
}

export const QUICK_START_STEPS: readonly QuickStartStep[] = [
  { index: 1, title: '安装工艺', navTarget: 'processes', checkApi: 'processes' },
  { index: 2, title: '配置触发器', navTarget: 'loops', checkApi: 'triggers' },
  { index: 3, title: '创建任务', navTarget: 'tasks', checkApi: 'tasks' },
  { index: 4, title: '监控执行', navTarget: 'memorial', checkApi: 'executions' },
  { index: 5, title: '验收产物', navTarget: 'loops', checkApi: 'artifacts' },
] as const;

/** 顶部 sticky Tab 的三个 key，与 ConceptNavPage 的 section id 对齐。 */
export const ONBOARDING_TABS: Array<{ key: string; label: string; icon: ReactNode }> = [
  { key: 'relation', label: '关系图', icon: <CompassOutlined /> },
  { key: 'concepts', label: '概念详解', icon: <AppstoreOutlined /> },
  { key: 'quickstart', label: '快速开始', icon: <ForwardOutlined /> },
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

/** 触发器 8 种类型 + 中文标签，用于环路详细说明区。 */
export const TRIGGER_TYPES: ReadonlyArray<{ type: string; label: string }> = [
  { type: 'manual', label: '手动触发' },
  { type: 'cron', label: '定时调度' },
  { type: 'webhook', label: 'Webhook' },
  { type: 'feishu_message', label: '飞书消息' },
  { type: 'feishu_command', label: '飞书命令' },
  { type: 'todo_completed', label: '事项完成' },
  { type: 'todo_state_changed', label: '事项状态变化' },
  { type: 'tag_added', label: '标签添加' },
] as const;

/** 门禁 4 种类型 + 中文标签，用于环路/事项详细说明区。 */
export const GATE_TYPES: ReadonlyArray<{ type: string; label: string }> = [
  { type: 'artifact_present', label: '产物存在' },
  { type: 'ai_criteria_review', label: 'AI 评审' },
  { type: 'human_approval', label: '人工审批' },
  { type: 'script_check', label: '脚本校验' },
] as const;

/** ThunderboltOutlined 给任务卡用（避免在 ConceptNode 里重复 import）。 */
export { ThunderboltOutlined } from '@ant-design/icons';
