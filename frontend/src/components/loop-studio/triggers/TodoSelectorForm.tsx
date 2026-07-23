// Todo 选择器表单（todo_completed / todo_state_changed 共用）。
// 存储格式：{"todo_id": 7}

import { useState, useCallback, useEffect, useMemo } from 'react';
import { Form, Select } from 'antd';
import * as db from '@/utils/database';
import type { Todo } from '@/types';

interface TodoSelectorFormProps {
  value: string;
  onChange: (json: string) => void;
  workspaceId?: number | null;
}

export function TodoSelectorForm({ value, onChange, workspaceId }: TodoSelectorFormProps) {
  const [todos, setTodos] = useState<Todo[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return { todo_id: v.todo_id ?? null };
    } catch {
      return { todo_id: null };
    }
  }, [value]);

  const [selectedId, setSelectedId] = useState<number | null>(parsed.todo_id);

  // 加载 todo 列表：按 loop 所属工作空间过滤（v1 纯 workspace-scoped，workspaceId 必须有值）
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

  const handleChange = useCallback((id: number | null) => {
    setSelectedId(id);
    if (id) {
      onChange(JSON.stringify({ todo_id: id }));
    } else {
      onChange('{}');
    }
  }, [onChange]);

  return (
    <Form.Item label="选择 Todo" tooltip="该 Todo 的状态变化将触发 Loop">
      <Select
        value={selectedId}
        onChange={handleChange}
        loading={loading}
        allowClear
        placeholder="选择一个 todo"
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
  );
}
