import { memo, useState, useEffect } from 'react';
import { Select, Input, Tooltip } from 'antd';
import { InfoCircleOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';

/**
 * 任务级执行模型选择器。
 *
 * 有可用模型（pi 的情况）→ Select(自带箭头,分组按 provider,只选不填)
 * 无可用模型（claudecode 等） → Input(手填兜底)
 *
 * - executor 变化时预拉可选模型。
 * - 留空 → 用执行器默认模型(executor.default_model);都没有则不传 --model。
 */
export const ModelPicker = memo(function ModelPicker({ model, executor, defaultModel, onChange }: {
  model: string | null;
  executor: string;
  defaultModel: string | null | undefined;
  onChange: (v: string | null) => void;
}) {
  const [models, setModels] = useState<string[]>([]);
  useEffect(() => {
    if (!executor) return;
    db.getExecutorModels(executor).then(setModels).catch(() => setModels([]));
  }, [executor]);

  const placeholder = defaultModel ? `留空用默认：${defaultModel}` : '留空用执行器自带配置';
  const label = (
    <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14, display: 'flex', alignItems: 'center', gap: 6 }}>
      执行模型
      <Tooltip title="指定本任务使用的模型，透传给对应执行器（如 claude --model、pi --model）。留空则用执行器默认模型；执行器也没配则不传 --model。">
        <InfoCircleOutlined style={{ color: 'var(--color-text-secondary)', fontSize: 12 }} />
      </Tooltip>
    </div>
  );

  // 无可用模型 → 普通输入框(手填)
  if (models.length === 0) {
    return (
      <div style={{ marginBottom: 16 }}>
        {label}
        <Input value={model ?? ''} placeholder={placeholder}
          onChange={(e) => onChange(e.target.value?.trim() || null)} allowClear />
      </div>
    );
  }

  // 有可用模型 → Select(自带+箭头+分组)
  // 按 provider 分组展示（如 minimax-anthropic / MiniMax-M3）
  const groups: Record<string, { label: string; value: string }[]> = {};
  models.forEach((full) => {
    const i = full.indexOf('/');
    const provider = i > 0 ? full.slice(0, i) : '其他';
    const mn = i > 0 ? full.slice(i + 1) : full;
    if (!groups[provider]) groups[provider] = [];
    groups[provider].push({ label: mn, value: full });
  });

  return (
    <div style={{ marginBottom: 16 }}>
      {label}
      <Select
        value={model || undefined}
        placeholder={placeholder}
        allowClear
        showSearch
        filterOption={(input: string, option?: { label: string; value: string }) =>
          (option?.label ?? '').toLowerCase().includes(input.toLowerCase())}
        onChange={(v: unknown) => onChange((v as string) || null)}
        style={{ width: '100%' }}
      >
        {Object.entries(groups).map(([provider, items]) => (
          <Select.OptGroup key={provider} label={provider}>
            {items.map((item) => (
              <Select.Option key={item.value} value={item.value}>{item.label}</Select.Option>
            ))}
          </Select.OptGroup>
        ))}
      </Select>
      {defaultModel && !model && (
        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginTop: 4 }}>
          将使用执行器默认模型：<b>{defaultModel}</b>
        </div>
      )}
    </div>
  );
});
