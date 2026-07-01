// Tag 选择器表单（tag_added 触发类型使用）。
// 存储格式：{"tag_id": 3}

import { useState, useCallback, useEffect, useMemo } from 'react';
import { Form, Select, Tag as AntTag } from 'antd';
import * as db from '@/utils/database';
import type { Tag } from '@/types';

interface TagSelectorFormProps {
  value: string;
  onChange: (json: string) => void;
}

export function TagSelectorForm({ value, onChange }: TagSelectorFormProps) {
  const [tags, setTags] = useState<Tag[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return { tag_id: v.tag_id ?? null };
    } catch {
      return { tag_id: null };
    }
  }, [value]);

  const [selectedId, setSelectedId] = useState<number | null>(parsed.tag_id);

  // 加载所有 tag
  useEffect(() => {
    setLoading(true);
    db.getAllTags()
      .then((list) => setTags(list))
      .catch(() => { /* 静默 */ })
      .finally(() => setLoading(false));
  }, []);

  const handleChange = useCallback((id: number | null) => {
    setSelectedId(id);
    if (id) {
      onChange(JSON.stringify({ tag_id: id }));
    } else {
      onChange('{}');
    }
  }, [onChange]);

  return (
    <Form.Item label="选择标签" tooltip="当该标签被添加到任意 todo 时触发 Loop">
      <Select
        value={selectedId}
        onChange={handleChange}
        loading={loading}
        allowClear
        placeholder="选择一个标签"
        showSearch
        filterOption={(input, option) =>
          (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
        }
        options={tags.map((t) => ({
          value: t.id,
          label: `${t.name}${t.color ? ` (${t.color})` : ''}`,
          color: t.color,
        }))}
        optionRender={(option) => (
          <span>
            {(option.data as any).color ? (
              <AntTag color={(option.data as any).color as string} style={{ marginRight: 6, lineHeight: '18px', fontSize: 11 }}>
                {option.data.label as string}
              </AntTag>
            ) : (
              option.data.label as string
            )}
          </span>
        )}
      />
    </Form.Item>
  );
}
