import { memo, useState } from 'react';
import { Input, Tooltip } from 'antd';
import { InfoCircleOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';

/**
 * 任务级执行模型选择器。
 *
 * MVP 形态：手填模型名 + 原生 datalist 下拉建议。
 * - 留空 → 用执行器默认模型（executor.default_model）；都没有则不传 --model（向后兼容）。
 * - 填值 → 任务级覆盖，优先级高于执行器默认。
 * - 下拉：focus 时按当前执行器懒加载可选模型（调其 models 子命令），
 *   能列模型的执行器（如 pi）给下拉建议，不能的退化为纯手填。
 *
 * 联动：展示当前执行器的默认模型作为 placeholder 提示，让用户知道"不填会用什么"。
 */
export const ModelPicker = memo(function ModelPicker({ model, executor, defaultModel, onChange }: {
  model: string | null;
  /** 当前选中的执行器名，用于按执行器拉取可选模型。 */
  executor: string;
  /** 当前执行器配置的默认模型，用作提示；无则提示"用执行器自带配置"。 */
  defaultModel: string | null | undefined;
  onChange: (v: string | null) => void;
}) {
  // 当前执行器可选模型（focus 时懒加载）。不能列模型的执行器返回空，退化为纯手填。
  const [models, setModels] = useState<string[]>([]);
  const fetchModels = async (name: string) => {
    if (!name) return;
    try {
      setModels(await db.getExecutorModels(name));
    } catch {
      // 拉取失败静默：手填兜底，不影响其它字段。
    }
  };

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
      {/* 能列模型的执行器（如 pi）给 datalist 下拉建议，不能的就是普通输入框。 */}
      <Input
        value={model ?? ''}
        placeholder={placeholder}
        list="todo-executor-models"
        onFocus={() => fetchModels(executor)}
        onChange={(e) => handle(e.target.value)}
        allowClear
      />
      {/* options 来自当前执行器的 models 子命令；空时不显示建议（退化为纯手填）。 */}
      <datalist id="todo-executor-models">
        {models.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>
    </div>
  );
});
