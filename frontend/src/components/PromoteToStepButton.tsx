// 「升级为环节」按钮 + 确认 Modal。
//
// 放在 todo 行内 (TodoList), 复制 todo 的 title/prompt/executor/acceptance_criteria
// 到 steps 表, 原 todo 保留, 不改变原 todo 的任何属性。

import { useState } from 'react';
import { Button, Modal, App } from 'antd';
import { ExperimentOutlined } from '@ant-design/icons';
import { promoteTodoToStep } from '@/utils/database/steps';
import { getAllTodos } from '@/utils/database/todos';
import { useApp } from '@/hooks/useApp';

interface PromoteToStepButtonProps {
  todoId: number;
  todoTitle: string;
}

/** 行内的「升级为环节」按钮, 点击打开 confirm modal。阻止冒泡避免触发 row click。 */
function PromoteTriggerButton({ todoTitle, onClick }: { todoTitle: string; onClick: (e: React.MouseEvent) => void }) {
  return (
    <Button
      type="text"
      size="small"
      icon={<ExperimentOutlined />}
      onClick={onClick}
      aria-label={`将「${todoTitle}」升级为环节`}
    >
      升级为环节
    </Button>
  );
}

/** Modal 内部的说明文本, 独立组件避免 PromoteConfirmModal 渲染块膨胀。 */
function PromoteModalBody({ todoTitle }: { todoTitle: string }) {
  return (
    <>
      <p style={{ marginTop: 0 }}>
        将 <b>「{todoTitle}」</b> 的标题、提示词、执行器、验收标准复制到环节表,
        之后可在 <b>环路编排 (Loop Studio) </b>中作为可复用的执行单元被任意 loop 引用。
      </p>
      <p style={{ color: 'var(--color-warning)', fontSize: 12, marginBottom: 0 }}>
        提示: 原 todo <b>不受影响</b>, 仍保留在「事项」列表中, 可继续使用。
        环节是独立实体, 创建后不能降级。
      </p>
    </>
  );
}

/** 二次确认 Modal, 受控: open / loading / todoTitle + onConfirm / onCancel。 */
function PromoteConfirmModal({
  open, loading, todoTitle, onConfirm, onCancel,
}: {
  open: boolean;
  loading: boolean;
  todoTitle: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal
      title="升级为环节"
      open={open}
      onCancel={onCancel}
      onOk={onConfirm}
      okText="确认升级"
      cancelText="取消"
      confirmLoading={loading}
      maskClosable={!loading}
      destroyOnClose
    >
      <PromoteModalBody todoTitle={todoTitle} />
    </Modal>
  );
}

/** 容器组件: 持有 modal 状态, 串联 promote 接口与全局 todo 列表刷新。 */
export function PromoteToStepButton({ todoId, todoTitle }: PromoteToStepButtonProps) {
  const { dispatch } = useApp();
  const { message } = App.useApp();
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);

  async function handleConfirm(): Promise<void> {
    setLoading(true);
    try {
      await promoteTodoToStep(todoId);
      message.success(`「${todoTitle}」已升级为环节`);
      setOpen(false);
      const todos = await getAllTodos();
      dispatch({ type: 'SET_TODOS', payload: todos });
    } catch (err) {
      const fallback = '请稍后重试';
      const detail = err instanceof Error ? err.message : typeof err === 'string' ? err : fallback;
      message.error('升级失败: ' + detail);
    } finally {
      setLoading(false);
    }
  }

  return (
    <>
      <PromoteTriggerButton todoTitle={todoTitle} onClick={(e) => { e.stopPropagation(); setOpen(true); }} />
      <PromoteConfirmModal
        open={open}
        loading={loading}
        todoTitle={todoTitle}
        onConfirm={handleConfirm}
        onCancel={() => !loading && setOpen(false)}
      />
    </>
  );
}