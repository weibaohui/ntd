// 评分控件组件：用于对执行记录进行评分。

import { useState, useEffect } from 'react';
import { Button, Popover, Space, InputNumber } from 'antd';
import { StarOutlined, StarFilled } from '@ant-design/icons';
import type { ExecutionRecord } from '@/types';

interface RatingControlProps {
  record: ExecutionRecord;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
}

export function RatingControl({ record, onRate }: RatingControlProps) {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState<number | null>(record.rating ?? null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    setValue(record.rating ?? null);
  }, [record.rating, record.id]);

  const handleSubmit = async (next: number | null) => {
    setSubmitting(true);
    try {
      await onRate(record.id, next);
      setOpen(false);
    } finally {
      setSubmitting(false);
    }
  };

  if (record.rating != null) {
    return (
      <Popover
        open={open}
        onOpenChange={setOpen}
        trigger="click"
        content={
          <Space.Compact style={{ width: 200 }}>
            <InputNumber
              min={0} max={100} value={value}
              onChange={v => setValue(typeof v === "number" ? v : null)}
              placeholder="0-100" style={{ width: "100%" }}
              onPressEnter={() => { if (value != null) handleSubmit(value); }}
            />
            <Button type="primary" loading={submitting} onClick={() => { if (value != null) handleSubmit(value); }}>
              更新
            </Button>
          </Space.Compact>
        }
      >
        <Button type="text" size="small" icon={<StarFilled style={{ color: "#faad14" }} />}>
          {record.rating}
        </Button>
      </Popover>
    );
  }

  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      trigger="click"
      content={
        <Space.Compact style={{ width: 200 }}>
          <InputNumber
            min={0} max={100} value={value}
            onChange={v => setValue(typeof v === "number" ? v : null)}
            placeholder="0-100" style={{ width: "100%" }}
            onPressEnter={() => { if (value != null) handleSubmit(value); }}
          />
          <Button type="primary" loading={submitting} onClick={() => { if (value != null) handleSubmit(value); }}>
            评分
          </Button>
        </Space.Compact>
      }
    >
      <Button type="text" size="small" icon={<StarOutlined />}>评分</Button>
    </Popover>
  );
}
