import { useState, useCallback } from 'react';
import { UnorderedListOutlined, PlusOutlined, EditOutlined, DeleteOutlined } from '@ant-design/icons';
import { Button, Popconfirm, App } from 'antd';
import { PageCard } from '../common/PageCard';
import { TodoList } from '../TodoList';
import { TodoDetail } from '../TodoDetail';
import { TodoDrawer } from '../TodoDrawer';
import { EmptyDetailPlaceholder } from '../EmptyDetailPlaceholder';
import { SIDEBAR_WIDTH } from '@/constants';
import { useApp } from '@/hooks/useApp';
import * as db from '@/utils/database';

interface TodoMobilePageProps {
  selectedTodoId: string | number | null;
  onOpenCreateModal: () => void;
  onSelectTodo: (todoId: string | number | null) => void;
  loopUpdateCount: number;
  onSelectLoop: (loopId: number) => void;
  onCreateLoop: () => void;
  forcedListMode?: 'item' | 'loop';
  onListModeChange: () => void;
  effectiveMobilePanel: 'list' | 'detail';
}

/**
 * 移动端事项页面组件
 * 列表页和详情页为两个独立的 PageCard 页面，各自有完整的标题栏
 * 列表页：PageCard 标题为"事项"
 * 详情页：PageCard 标题为具体事项标题，操作按钮在标题栏右侧
 */
export function TodoMobilePage({
  selectedTodoId,
  onOpenCreateModal,
  onSelectTodo,
  loopUpdateCount,
  onSelectLoop,
  onCreateLoop,
  forcedListMode,
  onListModeChange,
  effectiveMobilePanel,
}: TodoMobilePageProps) {
  const { state, dispatch } = useApp();
  const { message } = App.useApp();
  const { todos } = state;
  const selectedTodo = todos.find(t => t.id === selectedTodoId);
  const [todoDrawerOpen, setTodoDrawerOpen] = useState(false);

  const handleDelete = useCallback(async () => {
    if (!selectedTodo) return;
    try {
      await db.deleteTodo(selectedTodo.id);
      dispatch({ type: 'DELETE_TODO', payload: selectedTodo.id });
      dispatch({ type: 'SELECT_TODO', payload: null });
      message.success('删除成功');
    } catch {
      // ignore: interceptor already shows error
    }
  }, [selectedTodo, dispatch, message]);

  const listPage = (
    <PageCard
      icon={<UnorderedListOutlined />}
      title="事项"
      extra={
        <Button
          type="primary"
          size="small"
          icon={<PlusOutlined />}
          onClick={onOpenCreateModal}
        >
          新建
        </Button>
      }
      style={{ height: '100%' }}
      contentStyle={{ padding: 0, height: 'calc(100% - 43px)' }}
    >
      <TodoList
        onOpenCreateModal={onOpenCreateModal}
        onSelectTodo={onSelectTodo}
        loopUpdateCount={loopUpdateCount}
        onSelectLoop={onSelectLoop}
        onCreateLoop={onCreateLoop}
        forcedListMode={forcedListMode}
        onListModeChange={onListModeChange}
        hideCreateButton={true}
      />
    </PageCard>
  );

  const detailPage = selectedTodo ? (
    <PageCard
      title={selectedTodo.title}
      extra={
        <div style={{ display: 'flex', gap: 4 }}>
          <Button
            type="text"
            size="small"
            icon={<EditOutlined />}
            onClick={() => setTodoDrawerOpen(true)}
          />
          <Popconfirm title="删除任务" description="确定要删除吗？" onConfirm={handleDelete}>
            <Button type="text" size="small" icon={<DeleteOutlined />} />
          </Popconfirm>
        </div>
      }
      style={{ height: '100%' }}
      contentStyle={{ padding: 0, height: 'calc(100% - 43px)' }}
    >
      <TodoDetail hideTitleRow={true} />
      <TodoDrawer
        open={todoDrawerOpen}
        todo={selectedTodo}
        tags={state.tags}
        onClose={() => setTodoDrawerOpen(false)}
        onSaved={() => {
          // 只刷新当前 workspace 桶
          const wid = state.selectedWorkspace;
          if (wid != null) {
            db.getAllTodos(wid).then(todos => {
              dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: todos });
            });
          }
        }}
      />
    </PageCard>
  ) : (
    <PageCard
      style={{ height: '100%' }}
      contentStyle={{ padding: 0, height: 'calc(100% - 43px)' }}
    >
      <EmptyDetailPlaceholder />
    </PageCard>
  );

  return (
    <>
      <div
        className={effectiveMobilePanel === 'list' ? 'animate-fade-in' : ''}
        style={{
          width: SIDEBAR_WIDTH.mobile,
          flexShrink: 0,
          height: '100%',
          display: effectiveMobilePanel === 'list' ? 'block' : 'none',
        }}
      >
        {listPage}
      </div>
      <div
        className={effectiveMobilePanel === 'detail' ? 'animate-slide-in-right' : ''}
        style={{
          flex: 1,
          minWidth: 0,
          height: '100%',
          overflow: 'hidden',
          display: effectiveMobilePanel === 'detail' ? 'block' : 'none',
        }}
      >
        {detailPage}
      </div>
    </>
  );
}
