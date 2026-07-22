import { memo, useState, useEffect } from 'react';
import { AutoComplete, Tooltip } from 'antd';
import { InfoCircleOutlined, DownOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';

/**
 * 任务级执行模型选择器。
 *
 * Ant Design AutoComplete = 可输入+可选,没有下拉数据时就是普通输入框(手填兜底)。
 * - executor 变化时预拉可选模型,避免 focus 时才等 API。
 * - 留空 → 用执行器默认模型(executor.default_model);都没有则不传 --model(向后兼容)。
 * - 填值 → 任务级覆盖,优先级高于执行器默认。
 * - 联动:placeholder 展示当前执行器的默认模型。
 */
export const ModelPicker = memo(function ModelPicker({ model, executor, defaultModel, onChange }: {
  model: string | null;
  /** 当前选中的执行器名,用于按执行器拉取可选模型。 */
  executor: string;
  /** 当前执行器配置的默认模型,用作提示;无则提示"用执行器自带配置"。 */
  defaultModel: string | null | undefined;
  onChange: (v: string | null) => void;
}) {
  // 当前执行器可选模型。executor 变化时预拉;不能列模型的执行器返回空,退化为纯手填。
  const [models, setModels] = useState<string[]>([]);
  useEffect(() => {
    if (!executor) return;
    fetchModels(executor);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [executor]);

  const fetchModels = async (name: string) => {
    try {
      setModels(await db.getExecutorModels(name));
    } catch {
      setModels([]);
    }
  };

  const placeholder = defaultModel ? `留空用默认：${defaultModel}` : '留空用执行器自带配置';
  return (
    <div style={{ marginBottom: 16 }}>
      <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14, display: 'flex', alignItems: 'center', gap: 6 }}>
        执行模型
        <Tooltip title="指定本任务使用的模型，会透传给对应执行器（如 claude --model、pi --model）。留空则用执行器默认模型；执行器也没配则不传 --model。">
          <InfoCircleOutlined style={{ color: 'var(--color-text-secondary)', fontSize: 12 }} />
        </Tooltip>
      </div>
      <AutoComplete
        value={model ?? ''}
        options={models.map((m) => ({ value: m }))}
        placeholder={placeholder}
        onChange={(v: unknown) => {
          // AutoComplete 的 onChange 传的是选中的 value(string),不是 event。
          onChange((v as string)?.trim() || null);
        }}
        allowClear
        suffixIcon={<DownOutlined />}
        style={{ width: '100%' }}
      />
      {/* 当不填模型且执行器有默认模型时,提示用户会用什么。 */}
      {defaultModel && !model && (
        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginTop: 4 }}>
          将使用执行器默认模型：<b>{defaultModel}</b>
        </div>
      )}
    </div>
  );
});
