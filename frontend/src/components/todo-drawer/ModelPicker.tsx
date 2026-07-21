import { memo } from 'react';
import { Input, Tooltip } from 'antd';
import { InfoCircleOutlined } from '@ant-design/icons';

/**
 * 任务级执行模型选择器。
 *
 * MVP 形态：手填模型名，构建 argv 时透传给执行器的 --model flag。
 * - 留空 → 用执行器默认模型（executor.default_model）；都没有则不传 --model（向后兼容）。
 * - 填值 → 任务级覆盖，优先级高于执行器默认。
 *
 * 联动：展示当前执行器的默认模型作为 placeholder 提示，让用户知道"不填会用什么"，
 * 避免盲目填写与执行器配置冲突的模型名（如给 claude 填了 codex 专属模型）。
 */
export const ModelPicker = memo(function ModelPicker({ model, defaultModel, onChange }: {
  model: string | null;
  /** 当前执行器配置的默认模型，用作提示；无则提示"用执行器自带配置"。 */
  defaultModel: string | null | undefined;
  onChange: (v: string | null) => void;
}) {
  // 空输入归一为 null：让 reducer/后端按"未指定/清除任务级覆盖"处理。
  const handle = (v: string) => onChange(v.trim() || null);
  // placeholder：执行器配了默认模型就显示具体值，否则说明由执行器配置文件决定。
  const placeholder = defaultModel ? `留空用默认：${defaultModel}` : '留空用执行器自带配置';

  return (
    <div style={{ marginBottom: 16 }}>
      <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14, display: 'flex', alignItems: 'center', gap: 6 }}>
        执行模型
        <Tooltip title="指定本任务使用的模型，会透传给执行器（如 claude --model、pi --model）。留空则用执行器默认模型；执行器也没配则不传 --model。">
          <InfoCircleOutlined style={{ color: 'var(--color-text-secondary)', fontSize: 12 }} />
        </Tooltip>
      </div>
      <Input
        value={model ?? ''}
        placeholder={placeholder}
        onChange={(e) => handle(e.target.value)}
        allowClear
      />
    </div>
  );
});
