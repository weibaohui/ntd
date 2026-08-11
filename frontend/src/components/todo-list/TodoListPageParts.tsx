// TodoListPageParts — TodoListPage 的拆分子模块（响应 028 PR review 的函数体 ≤30 行 规范）。
//
// 拆分原则：把 header JSX、行操作回调、带参执行 Modal 三块独立成组件/hook，
// 让 TodoListPage 主函数仅负责组合，函数体保持简短。
//
// 1. TodoListHeader：桌面/移动端顶部 header（搜索 + 刷新 + Segmented + 新建）
// 2. useTodoRowActions：单行执行/删除/带参执行 + Modal state
// 3. ExecuteWithArgsModal：带参执行 Modal（受控组件，由 useTodoRowActions 驱动）

import { useCallback, useState, type ReactNode } from 'react';
import { Button, Input, Modal, Segmented, message } from 'antd';
import {
  AppstoreOutlined,
  PlusOutlined,
  ProjectOutlined,
  ReloadOutlined,
  SearchOutlined,
  ThunderboltOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons';
import * as db from '@/utils/database';
import type { TodoCenterItem } from '@/types';

interface TodoListHeaderProps {
  isMobile: boolean;
  /** 视图模式：card 卡片墙 / list 表格 / kanban 看板 / running 执行监控（复用 RunningBoard）。 */
  viewMode: 'card' | 'list' | 'kanban' | 'running';
  searchKeyword: string;
  loading: boolean;
  onSearchChange: (kw: string) => void;
  onViewChange: (m: 'card' | 'list' | 'kanban' | 'running') => void;
  onReload: () => void;
  onCreate: () => void;
}

/**
 * 顶部 header：搜索框 + 刷新 + Segmented + 新建。
 * 桌面端展开全部；移动端精简（去掉搜索/刷新，保留 Segmented + 新建）。
 * 拆出独立组件避免 TodoListPage 主函数膨胀。
 */
export function TodoListHeader({
  isMobile,
  viewMode,
  searchKeyword,
  loading,
  onSearchChange,
  onViewChange,
  onReload,
  onCreate,
}: TodoListHeaderProps): ReactNode {
  // 公共 Segmented + 新建按钮：移动端/桌面端共用
  const segmented = (
    <Segmented
      size="small"
      value={viewMode}
      onChange={(v) => onViewChange(v as 'card' | 'list' | 'kanban' | 'running')}
      options={[
        { value: 'card', icon: <AppstoreOutlined />, title: isMobile ? '卡片' : '卡片视图' },
        { value: 'list', icon: <UnorderedListOutlined />, title: '列表' },
        // 看板视图：复用 KanbanBoard（todo 维度状态流转 + 拖拽改状态），与任务页看板形态对齐。
        { value: 'kanban', icon: <ProjectOutlined />, title: isMobile ? '看板' : '看板视图' },
        // 执行监控：复用 RunningBoard（执行记录 6 列 + 实时 WS + 评审流水线），与 card/kanban 的 todo 定义维度区分。
        { value: 'running', icon: <ThunderboltOutlined />, title: isMobile ? '运行' : '执行监控' },
      ]}
      data-testid="todo-center-view-toggle"
    />
  );
  const createBtn = (
    <Button size="small" type="primary" icon={<PlusOutlined />} onClick={onCreate}>
      新建
    </Button>
  );

  // 精简 header：移动端或看板/运行态，只保留 Segmented + 新建。
  // 看板态：KanbanBoard 自带顶栏（搜索/时间窗/项目过滤/统计）；运行态：RunningBoard 自带统计栏+刷新+实时 WS。
  if (isMobile || viewMode === 'kanban' || viewMode === 'running') {
    return (
      <>
        {segmented}
        {createBtn}
      </>
    );
  }

  // 桌面端：搜索 + 刷新 + Segmented + 新建
  return (
    <>
      <Input
        allowClear
        size="small"
        placeholder="搜索标题或 Prompt"
        prefix={<SearchOutlined />}
        value={searchKeyword}
        onChange={(e) => onSearchChange(e.target.value)}
        style={{ width: 200 }}
        data-testid="items-page-search"
      />
      <Button size="small" icon={<ReloadOutlined />} onClick={onReload} loading={loading} aria-label="刷新">
        刷新
      </Button>
      {segmented}
      {createBtn}
    </>
  );
}

interface UseTodoRowActionsArgs {
  workspaceId: number | null;
  onReload: () => void;
}

/**
 * 事项行操作：单行执行 / 删除（二次确认）/ 带参执行 Modal。
 * 拆成 hook 让 TodoListPage 主函数保持简短，便于测试与复用。
 */
export function useTodoRowActions({ workspaceId, onReload }: UseTodoRowActionsArgs) {
  // 带参执行 Modal state
  const [executeWithArgsModalOpen, setExecuteWithArgsModalOpen] = useState(false);
  const [executeArgs, setExecuteArgs] = useState('');
  const [pendingExecuteTodo, setPendingExecuteTodo] = useState<TodoCenterItem | null>(null);

  // 单行执行：不携带 params（即「立即执行」），带参执行走 handleExecuteWithArgs
  const handleExecuteTodo = useCallback(async (todo: TodoCenterItem) => {
    if (workspaceId == null) return;
    try {
      await db.executeTodo(workspaceId, todo.id, todo.executor || undefined);
      message.success('任务已开始执行');
      onReload();
    } catch (e) {
      message.error(`执行失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, onReload]);

  // 单行删除：破坏性操作先二次确认，避免行菜单误触导致不可恢复
  const handleDeleteTodo = useCallback((todo: TodoCenterItem) => {
    if (workspaceId == null) return;
    Modal.confirm({
      title: `确认删除「${todo.title}」？`,
      content: '删除后不可恢复。',
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await db.deleteTodo(workspaceId, todo.id);
          message.success('已删除');
          onReload();
        } catch (e) {
          message.error(`删除失败: ${e instanceof Error ? e.message : '未知错误'}`);
        }
      },
    });
  }, [workspaceId, onReload]);

  // 带参执行：弹 Modal 让用户输入补充信息
  const handleExecuteWithArgs = useCallback((todo: TodoCenterItem) => {
    setPendingExecuteTodo(todo);
    setExecuteArgs('');
    setExecuteWithArgsModalOpen(true);
  }, []);

  // 确认带参执行：params.message 字段与后端 ExecuteRequest 对齐
  const confirmExecuteWithArgs = useCallback(async () => {
    if (!pendingExecuteTodo || workspaceId == null) return;
    const params = executeArgs.trim() ? { message: executeArgs.trim() } : undefined;
    try {
      await db.executeTodo(workspaceId, pendingExecuteTodo.id, pendingExecuteTodo.executor || undefined, params);
      message.success('任务已开始执行');
      setExecuteWithArgsModalOpen(false);
      setPendingExecuteTodo(null);
      onReload();
    } catch (e) {
      message.error(`执行失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [pendingExecuteTodo, workspaceId, executeArgs, onReload]);

  // 关闭带参执行 Modal
  const cancelExecuteWithArgs = useCallback(() => {
    setExecuteWithArgsModalOpen(false);
    setPendingExecuteTodo(null);
  }, []);

  return {
    handleExecuteTodo,
    handleDeleteTodo,
    handleExecuteWithArgs,
    executeWithArgsModalOpen,
    executeArgs,
    setExecuteArgs,
    confirmExecuteWithArgs,
    cancelExecuteWithArgs,
  };
}

interface ExecuteWithArgsModalProps {
  open: boolean;
  args: string;
  onArgsChange: (v: string) => void;
  onOk: () => void;
  onCancel: () => void;
}

/** 带参执行 Modal：受控组件，由 useTodoRowActions 驱动。 */
export function ExecuteWithArgsModal({
  open, args, onArgsChange, onOk, onCancel,
}: ExecuteWithArgsModalProps) {
  return (
    <Modal
      title={<><ThunderboltOutlined style={{ marginRight: 8 }} />带参执行</>}
      open={open}
      onOk={onOk}
      onCancel={onCancel}
      okText="开始执行"
      cancelText="取消"
      destroyOnHidden
    >
      <p style={{ marginBottom: 12, color: 'var(--color-text-secondary)' }}>
        输入补充信息，将与任务原有内容一起执行：
      </p>
      <Input.TextArea
        value={args}
        onChange={(e) => onArgsChange(e.target.value)}
        rows={4}
        placeholder="输入补充信息..."
      />
    </Modal>
  );
}
