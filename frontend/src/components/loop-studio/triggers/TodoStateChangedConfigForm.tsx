// Todo 状态变更配置表单（todo_state_changed 专用）：选择 todo 和过滤条件。
// 存储格式：{"todo_id":7,"to_status":"completed"}

import { useState, useEffect, useMemo } from 'react';
import { Form, Select } from 'antd';
import * as db from '@/utils/database';
import type { Todo } from '@/types';

interface TodoStateChangedConfigFormProps {
  value: string;
  onChange: (json: string) => void;
  workspaceId?: number | null;
}

export function TodoStateChangedConfigForm({ value, onChange, workspaceId }: TodoStateChangedConfigFormProps) {
  const [todos, setTodos] = useState<Todo[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return {
        todo_id: v.todo_id ?? null,
        to_status: v.to_status ?? '',
      };
    } catch {
      return { todo_id: null, to_status: '' };
    }
  }, [value]);

  const [todoId, setTodoId] = useState<number | null>(parsed.todo_id);
  const [toStatus, setToStatus] = useState(parsed.to_status);

  useEffect(() => {
    if (workspaceId == null) {
      setTodos([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    db.getAllTodos(workspaceId)
      .then((list) => setTodos(list))
      .catch(() => { /* 静默 */ })
      .finally(() => setLoading(false));
  }, [workspaceId]);

  useEffect(() => {
    const config: Record<string, any> = {};
    if (todoId) config.todo_id = todoId;
    if (toStatus) config.to_status = toStatus;
    onChange(JSON.stringify(config));
  }, [todoId, toStatus, onChange]);

  return (
    <div>
      <Form.Item label="选择 Todo" tooltip="该 Todo 的状态变化将触发 Loop（留空=任意 Todo 都触发）">
        <Select
          value={todoId}
          onChange={(v) => setTodoId(v)}
          loading={loading}
          allowClear
          placeholder="选择一个 todo（留空=任意）"
          showSearch
          filterOption={(input, option) =>
            (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
          }
          options={todos.map((t) => ({
            value: t.id,
            label: t.title,
          }))}
        />
      </Form.Item>
      <Form.Item label="目标状态" tooltip="当 Todo 变更为该状态时触发（留空=任意状态变更均触发）">
        <Select
          value={toStatus}
          onChange={(v) => setToStatus(v)}
          allowClear
          placeholder="选择目标状态（留空=任意）"
          options={[
            { value: 'pending', label: 'pending（待执行）' },
            { value: 'running', label: 'running（执行中）' },
            { value: 'completed', label: 'completed（已完成）' },
            { value: 'failed', label: 'failed（失败）' },
            { value: 'cancelled', label: 'cancelled（已取消）' },
          ]}
        />
      </Form.Item>
    </div>
  );
}
