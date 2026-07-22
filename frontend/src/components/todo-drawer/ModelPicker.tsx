import { memo, useState, useRef } from 'react';
import { Select, Input, Tooltip } from 'antd';
import { InfoCircleOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';

// 已知能通过 models 子命令动态列模型的执行器，和后端 list_models match 分支保持一致。
const EXECUTORS_WITH_MODELS = ['pi', 'mimo', 'opencode', 'kilo'];

/**
 * 任务级执行模型选择器。
 *
 * - 能列模型的执行器（pi/mimo/opencode/kilo）→ Select(分组按 provider,只选不填)
 * - 不能列模型的执行器（claudecode 等）→ Input(手填兜底)
 * - 模型列表在首次展开下拉时异步加载(懒加载,只拉一次)。
 */
export const ModelPicker = memo(function ModelPicker({ model, executor, defaultModel, onChange }: {
  model: string | null;
  executor: string;
  defaultModel: string | null | undefined;
  onChange: (v: string | null) => void;
}) {
  const [models, setModels] = useState<string[]>([]);
  const fetchedRef = useRef(false);
  const fetchModels = async () => {
    if (!executor || fetchedRef.current) return;
    fetchedRef.current = true;
    try {
      setModels(await db.getExecutorModels(executor));
    } catch {
      setModels([]);
    }
  };

  const supportsModels = EXECUTORS_WITH_MODELS.includes(executor);
  const placeholder = defaultModel ? `留空用默认：${defaultModel}` : '留空用执行器自带配置';
  const label = (
    <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14, display: 'flex', alignItems: 'center', gap: 6 }}>
      执行模型
      <Tooltip title="指定本任务使用的模型，透传给对应执行器。留空则用执行器默认模型；执行器也没配则不传 --model。">
        <InfoCircleOutlined style={{ color: 'var(--color-text-secondary)', fontSize: 12 }} />
      </Tooltip>
    </div>
  );

  // 不能列模型的执行器 → 普通输入框(手填)
  if (!supportsModels) {
    return (
      <div style={{ marginBottom: 16 }}>
        {label}
        <Input value={model ?? ''} placeholder={placeholder}
          onChange={(e) => onChange(e.target.value?.trim() || null)} allowClear />
      </div>
    );
  }

  // 能列模型的执行器 → Select(自带箭头,展开时懒加载)
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
        notFoundContent={fetchedRef.current && models.length === 0 ? '暂无可选模型' : !fetchedRef.current ? '点击展开加载模型列表...' : undefined}
        onDropdownVisibleChange={(open) => { if (open) fetchModels(); }}
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
