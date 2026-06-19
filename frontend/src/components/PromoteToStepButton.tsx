// 「升级为环节」按钮 + 确认 Modal。
//
// 放在 todo 行内 (TodoList), 只在 kind !== 'step' 时显示,
// 因为已经升级过的 todo 不需要重复升级, 环节视图里通常走降级路径。
//
// 为什么不直接走环节新建流程: 现实场景中, 用户往往先在一个 todo 上把 prompt /
// 验收标准 / 工作空间打磨好, 然后才意识到「我以后还要复用」。升级路径比从头新建
// 更自然, 也保留了原 todo 的全部历史 (执行记录、tag、调度配置)。
//
// 文件结构拆 3 块, 让每个组件函数体 < 30 行:
// - PromoteTriggerButton: 行内的升级按钮 (presentational)
// - PromoteConfirmModal:  二次确认弹窗 (presentational)
// - PromoteToStepButton: 拥有状态与副作用的容器, 组合前两者

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
        将 <b>「{todoTitle}」</b> 从「事项」提升为「环节」, 之后可在
        <b> 环路编排 (Loop Studio) </b>中作为可复用的执行单元被任意 loop 引用。
      </p>
      <p style={{ color: 'var(--color-text-secondary)' }}>升级后典型用法:</p>
      <ul style={{ color: 'var(--color-text-secondary)', paddingLeft: 20, marginTop: 4 }}>
        <li>在 Loop Studio 新建 loop, 直接把此环节加入执行队列</li>
        <li>被多个 loop 共享, 触发时顺序复用同一份 prompt 与配置</li>
      </ul>
      <p style={{ color: 'var(--color-warning)', fontSize: 12, marginBottom: 0 }}>
        提示: 此操作会保留 todo 的全部历史 (执行记录 / tag / 调度), 但
        <b> 升级后此 todo 不再出现在「事项」过滤下</b>, 如需恢复可在「环节」列表降级。
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