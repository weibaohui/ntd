import { memo, useState, useRef, useEffect } from 'react';
import { Select, Input, Tooltip } from 'antd';
import { InfoCircleOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';

/**
 * 任务级执行模型选择器。
 *
 * - supportsModels=true（pi/mimo/opencode/kilo）→ Select(分组按 provider,只选不填)
 * - supportsModels=false（claudecode 等）→ Input(手填兜底)
 * - 模型列表在首次展开下拉时异步加载(懒加载,只拉一次);executor 变化时重置。
 * - supportsModels 来自后端 ExecutorConfig.supports_models（单一事实来源，不再前端硬编码）。
 */
export const ModelPicker = memo(function ModelPicker({ model, executor, supportsModels, defaultModel, onChange }: {
  model: string | null;
  executor: string;
  /** 当前执行器是否支持动态列模型（来自后端 supports_models）。 */
  supportsModels: boolean;
  defaultModel: string | null | undefined;
  onChange: (v: string | null) => void;
}) {
  const [models, setModels] = useState<string[]>([]);
  const fetchedRef = useRef(false);
  // 跟踪当前活跃的执行器名称，在异步请求完成后检查是否已过期。
  // 防止执行器快速切换时旧请求的响应覆盖新模型列表。
  const activeExecutorRef = useRef(executor);
  // executor 变化时重置状态，下次展开下拉重新拉取对应执行器的模型列表。
  useEffect(() => {
    setModels([]);
    fetchedRef.current = false;
    activeExecutorRef.current = executor;
  }, [executor]);
  const fetchModels = async () => {
    if (!executor || fetchedRef.current) return;
    fetchedRef.current = true;
    // 在 await 前捕获当前执行器，之后用 activeExecutorRef（跟随最新 render）对比而非闭包内的 executor。
    // 闭包 executor 不会随 re-render 更新，用 ref 才能获取到最新值。
    const requestedFor = executor;
    try {
      const result = await db.getExecutorModels(executor);
      // 请求完成时检查执行器是否已切换：若当前活跃执行器已不是发起请求的那个，丢弃结果。
      if (activeExecutorRef.current !== requestedFor) return;
      setModels(result);
    } catch {
      if (activeExecutorRef.current !== requestedFor) return;
      setModels([]);
    }
  };

  const placeholder = defaultModel ? `留空用默认：${defaultModel}` : '留空用执行器自带配置';
  const label = (
    <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14, display: 'flex', alignItems: 'center', gap: 6 }}>
      执行模型
      <Tooltip title="指定本任务使用的模型，透传给对应执行器。留空则用执行器默认模型；执行器也没配则不传 --model。">
        <InfoCircleOutlined style={{ color: 'var(--color-text-secondary)', fontSize: 12 }} />
      </Tooltip>
    </div>
  );

  // 不支持动态列模型 → 普通输入框（手填）
  if (!supportsModels) {
    return (
      <div style={{ marginBottom: 16 }}>
        {label}
        <Input value={model ?? ''} placeholder={placeholder}
          onChange={(e) => onChange(e.target.value?.trim() || null)} allowClear />
      </div>
    );
  }

  // 支持动态列模型 → Select（展开时懒加载）
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
        notFoundContent={fetchedRef.current && models.length === 0 ? '暂无可选模型' : !fetchedRef.current ? '加载中，请稍后...' : undefined}
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
