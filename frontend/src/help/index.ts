// 帮助内容注册表入口。
//
// 设计要点：
// 1. HELP_PAGES 是全量注册表，新增页面/功能点只需在此追加一行。
// 2. 顺序即树形展示顺序，按 LeftRail 分组（概览/工作/观察/配置）排列。
// 3. 每个功能点的 docFile 指向 help/pages/ 下的骨架 md，M3 阶段逐个填充。

import type { HelpPage } from './types';

/**
 * 全量帮助页面注册表。
 *
 * 顺序即树形展示顺序。'_overview' 是虚拟页面，对应帮助首页。
 * pageId 与 useHelpContent.viewToPageId 的派生值一一对应。
 */
export const HELP_PAGES: HelpPage[] = [
  // ===== 概览 =====
  {
    pageId: '_overview',
    title: '帮助首页',
    overviewDoc: '_overview.md',
    features: [],
  },
  {
    pageId: 'dashboard',
    title: '仪表盘',
    overviewDoc: 'dashboard.md',
    features: [
      { id: 'dashboard-tab-switch', title: 'Tab 切换（7 个语义域）', docFile: 'dashboard-tab-switch.md' },
      { id: 'dashboard-time-range', title: '全局时间范围选择', docFile: 'dashboard-time-range.md' },
      { id: 'dashboard-overview', title: '总览 Tab', docFile: 'dashboard-overview.md' },
      { id: 'dashboard-cost', title: '成本与模型 Tab', docFile: 'dashboard-cost.md' },
      { id: 'dashboard-automation', title: '自动化 Tab', docFile: 'dashboard-automation.md' },
      { id: 'dashboard-process', title: '工艺 Tab', docFile: 'dashboard-process.md' },
    ],
  },
  {
    pageId: 'onboarding',
    title: '导航（概念首页）',
    overviewDoc: 'onboarding.md',
    features: [
      { id: 'onboarding-concept-graph', title: '概念关系图', docFile: 'onboarding-concept-graph.md' },
      { id: 'onboarding-quick-start', title: '快速开始 5 步', docFile: 'onboarding-quick-start.md' },
    ],
  },

  // ===== 工作 =====
  {
    pageId: 'tasks-list',
    title: '任务（列表）',
    overviewDoc: 'tasks-list.md',
    features: [
      { id: 'tasks-create', title: '新建任务', docFile: 'tasks-create.md' },
      { id: 'tasks-view-switch', title: '视图切换（列表/看板/卡片）', docFile: 'tasks-view-switch.md' },
      { id: 'tasks-search', title: '搜索任务', docFile: 'tasks-search.md' },
      { id: 'tasks-time-filter', title: '时间窗过滤', docFile: 'tasks-time-filter.md' },
      { id: 'tasks-batch-delete', title: '批量删除任务', docFile: 'tasks-batch-delete.md' },
      { id: 'tasks-refresh', title: '刷新', docFile: 'tasks-refresh.md' },
    ],
  },
  {
    pageId: 'tasks-detail',
    title: '任务（详情）',
    overviewDoc: 'tasks-detail.md',
    features: [
      { id: 'task-detail-back', title: '返回列表', docFile: 'task-detail-back.md' },
      { id: 'task-detail-title', title: '动态详情标题', docFile: 'task-detail-title.md' },
      { id: 'task-detail-dag', title: '查看任务 DAG / 执行历史', docFile: 'task-detail-dag.md' },
      { id: 'task-detail-open-todo', title: 'DAG 节点跳转事项', docFile: 'task-detail-open-todo.md' },
    ],
  },
  {
    pageId: 'todos-list',
    title: '事项（列表）',
    overviewDoc: 'todos-list.md',
    features: [
      { id: 'todo-list-create', title: '新建事项', docFile: 'todo-list-create.md' },
      { id: 'todo-list-view-switch', title: '卡片/列表视图切换', docFile: 'todo-list-view-switch.md' },
      { id: 'todo-list-search', title: '搜索过滤', docFile: 'todo-list-search.md' },
      { id: 'todo-list-refresh', title: '刷新列表', docFile: 'todo-list-refresh.md' },
      { id: 'todo-list-row-actions', title: '单行操作（删除/执行/带参执行）', docFile: 'todo-list-row-actions.md' },
      { id: 'todo-list-execute-with-args', title: '带参执行 Modal', docFile: 'todo-list-execute-with-args.md' },
    ],
  },
  {
    pageId: 'todos-detail',
    title: '事项（详情）',
    overviewDoc: 'todos-detail.md',
    features: [
      { id: 'todo-detail-back', title: '返回列表', docFile: 'todo-detail-back.md' },
      { id: 'todo-detail-title-optimize', title: '自动优化标题', docFile: 'todo-detail-title-optimize.md' },
      { id: 'todo-detail-edit', title: '编辑事项', docFile: 'todo-detail-edit.md' },
      { id: 'todo-detail-delete', title: '删除事项', docFile: 'todo-detail-delete.md' },
      { id: 'todo-detail-open-post', title: '打开帖子（执行记录）', docFile: 'todo-detail-open-post.md' },
    ],
  },
  {
    pageId: 'loops-list',
    title: '环路（列表）',
    overviewDoc: 'loops-list.md',
    features: [
      { id: 'loop-list-search', title: '搜索环路', docFile: 'loop-list-search.md' },
      { id: 'loop-list-refresh', title: '刷新列表', docFile: 'loop-list-refresh.md' },
      { id: 'loop-list-open-config', title: '打开工作空间环路配置页', docFile: 'loop-list-open-config.md' },
      { id: 'loop-list-delete', title: '删除环路', docFile: 'loop-list-delete.md' },
      { id: 'loop-list-toggle-status', title: '启停切换', docFile: 'loop-list-toggle-status.md' },
      { id: 'loop-list-select', title: '跳转环路详情', docFile: 'loop-list-select.md' },
    ],
  },
  {
    pageId: 'loops-detail',
    title: '环路（详情）',
    overviewDoc: 'loops-detail.md',
    features: [
      { id: 'loop-detail-back', title: '返回列表', docFile: 'loop-detail-back.md' },
      { id: 'loop-detail-delete', title: '删除环路', docFile: 'loop-detail-delete.md' },
      { id: 'loop-detail-toggle-status', title: '启停切换', docFile: 'loop-detail-toggle-status.md' },
      { id: 'loop-detail-open-process', title: '跳转来源工艺', docFile: 'loop-detail-open-process.md' },
      { id: 'loop-detail-steps-expand', title: '步骤展开/执行环节查看', docFile: 'loop-detail-steps-expand.md' },
      { id: 'loop-detail-open-todo', title: '流程图节点跳转事项', docFile: 'loop-detail-open-todo.md' },
    ],
  },
  {
    pageId: 'processes',
    title: '工艺',
    overviewDoc: 'processes.md',
    features: [
      { id: 'process-create', title: '创建工艺', docFile: 'process-create.md' },
      { id: 'process-scope-switch', title: '我的/模板视图切换', docFile: 'process-scope-switch.md' },
      { id: 'process-install', title: '安装到工作空间', docFile: 'process-install.md' },
      { id: 'process-detail', title: '查看工艺详情', docFile: 'process-detail.md' },
      { id: 'process-edit', title: '编辑工艺（进编辑器）', docFile: 'process-edit.md' },
      { id: 'process-copy', title: '复制为我的工艺', docFile: 'process-copy.md' },
    ],
  },

  // ===== 观察 =====
  {
    pageId: 'messages',
    title: '消息',
    overviewDoc: 'messages.md',
    features: [
      { id: 'messages-refresh', title: '刷新消息列表', docFile: 'messages-refresh.md' },
      { id: 'messages-config', title: '打开消息配置抽屉', docFile: 'messages-config.md' },
      { id: 'messages-bot-filter', title: '按 Bot 筛选', docFile: 'messages-bot-filter.md' },
      { id: 'messages-view-detail', title: '查看消息详情', docFile: 'messages-view-detail.md' },
      { id: 'messages-view-execution', title: '查看执行记录', docFile: 'messages-view-execution.md' },
      { id: 'messages-view-loop-execution', title: '查看环路执行详情', docFile: 'messages-view-loop-execution.md' },
    ],
  },
  {
    pageId: 'blackboard',
    title: '黑板',
    overviewDoc: 'blackboard.md',
    features: [
      { id: 'blackboard-refresh', title: '刷新黑板', docFile: 'blackboard-refresh.md' },
      { id: 'blackboard-settings', title: '打开黑板设置', docFile: 'blackboard-settings.md' },
      { id: 'blackboard-queue', title: '查看待处理队列', docFile: 'blackboard-queue.md' },
      { id: 'blackboard-topic-toolbar', title: '主题级操作（生成建议/删除）', docFile: 'blackboard-topic-toolbar.md' },
      { id: 'blackboard-debounce-bar', title: '防抖双进度条', docFile: 'blackboard-debounce-bar.md' },
      { id: 'blackboard-wiki-layout', title: 'Wiki 布区切换', docFile: 'blackboard-wiki-layout.md' },
    ],
  },
  {
    pageId: 'memorial',
    title: '看板',
    overviewDoc: 'memorial.md',
    features: [
      { id: 'board-mode-switch', title: '视图模式切换（四视图）', docFile: 'board-mode-switch.md' },
      { id: 'board-search', title: '搜索任务', docFile: 'board-search.md' },
      { id: 'board-time-filter', title: '时间窗过滤', docFile: 'board-time-filter.md' },
      { id: 'board-run-history-switch', title: '切换历史运行记录', docFile: 'board-run-history-switch.md' },
      { id: 'board-expand-toggle', title: '展开/收起卡片详情', docFile: 'board-expand-toggle.md' },
      { id: 'board-open-todo', title: '跳转事项详情', docFile: 'board-open-todo.md' },
    ],
  },

  // ===== 配置 =====
  {
    pageId: 'settings-skills',
    title: '技能',
    overviewDoc: 'settings-skills.md',
    features: [
      { id: 'skills-view-switch', title: '子视图切换（总览/技能市场/版本更新）', docFile: 'skills-view-switch.md' },
      { id: 'skills-overview', title: '总览视图', docFile: 'skills-overview.md' },
      { id: 'skills-marketplace', title: '技能市场视图', docFile: 'skills-marketplace.md' },
      { id: 'skills-version-update', title: '版本更新视图', docFile: 'skills-version-update.md' },
    ],
  },
  {
    pageId: 'settings-experts',
    title: '专家',
    overviewDoc: 'settings-experts.md',
    features: [
      { id: 'experts-import', title: '导入专家', docFile: 'experts-import.md' },
      { id: 'experts-reload', title: '重新加载专家', docFile: 'experts-reload.md' },
      { id: 'experts-search', title: '搜索专家', docFile: 'experts-search.md' },
      { id: 'experts-create', title: 'AI 创建专家', docFile: 'experts-create.md' },
      { id: 'experts-detail', title: '打开专家详情', docFile: 'experts-detail.md' },
      { id: 'experts-tabs', title: '专家/团队 Tab 切换', docFile: 'experts-tabs.md' },
    ],
  },
  {
    pageId: 'settings-executors',
    title: '执行器',
    overviewDoc: 'settings-executors.md',
    features: [
      { id: 'executors-tabs', title: '执行器/API Key/正在运行/会话 Tab', docFile: 'executors-tabs.md' },
      { id: 'executors-batch-detect', title: '批量检测执行器', docFile: 'executors-batch-detect.md' },
      { id: 'executors-row-actions', title: '单执行器操作（设默认/检测/修复/安装/测试）', docFile: 'executors-row-actions.md' },
      { id: 'executors-toggle-enabled', title: '启用/禁用执行器开关', docFile: 'executors-toggle-enabled.md' },
      { id: 'executors-running-stop', title: '正在运行 Tab 批量停止', docFile: 'executors-running-stop.md' },
      { id: 'executors-usage-stats', title: 'AI 使用统计开关 + Cron', docFile: 'executors-usage-stats.md' },
    ],
  },
  {
    pageId: 'settings-bots',
    title: '智能助手',
    overviewDoc: 'settings-bots.md',
    features: [
      { id: 'assistant-refresh', title: '刷新智能助手列表', docFile: 'assistant-refresh.md' },
      { id: 'assistant-bind', title: '绑定智能助手（飞书二维码）', docFile: 'assistant-bind.md' },
      { id: 'assistant-open-config', title: '打开配置抽屉', docFile: 'assistant-open-config.md' },
      { id: 'assistant-toggle-enabled', title: '启用/禁用切换', docFile: 'assistant-toggle-enabled.md' },
      { id: 'assistant-delete', title: '删除 Bot', docFile: 'assistant-delete.md' },
    ],
  },
  {
    pageId: 'settings-pd',
    title: '工作空间',
    overviewDoc: 'settings-pd.md',
    features: [
      { id: 'ws-add', title: '新建工作空间', docFile: 'ws-add.md' },
      { id: 'ws-rename', title: '编辑工作空间名称', docFile: 'ws-rename.md' },
      { id: 'ws-toggle-worktree', title: 'Git Worktree / 自动清理开关', docFile: 'ws-toggle-worktree.md' },
      { id: 'ws-delete', title: '删除工作空间', docFile: 'ws-delete.md' },
      { id: 'ws-bot-count', title: '查看绑定智能体数量', docFile: 'ws-bot-count.md' },
      { id: 'ws-prompt-modal', title: '编辑基础约定', docFile: 'ws-prompt-modal.md' },
    ],
  },
  {
    pageId: 'settings-more',
    title: '更多设置',
    overviewDoc: 'settings-more.md',
    features: [
      { id: 'settings-tab-system', title: '系统设置 Tab', docFile: 'settings-tab-system.md' },
      { id: 'settings-tab-interface', title: '界面显示 Tab', docFile: 'settings-tab-interface.md' },
      { id: 'settings-tab-tags', title: '标签管理 Tab', docFile: 'settings-tab-tags.md' },
      { id: 'settings-tab-templates', title: '模板管理 Tab', docFile: 'settings-tab-templates.md' },
      { id: 'settings-tab-backup', title: '备份与恢复 Tab', docFile: 'settings-tab-backup.md' },
      { id: 'settings-tab-cloud-sync', title: '云端同步 Tab', docFile: 'settings-tab-cloud-sync.md' },
    ],
  },
];
