// PhasePropertyForm.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：阶段属性面板（PhasePropertyForm）。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.10 + 设计 §7.2）：
// - 暴露 3 个字段：id/name/spec（验收标准已归环节，见 LinkPropertyForm）
// - 每个字段 onChange → updatePhaseField → onDefinitionChange
//
// 数据流（M4 单向）：
//   definition → 找到 phase → 渲染表单
//   字段修改 → updatePhaseField → onDefinitionChange(newDefinition)
// ---------------------------------------------------------------------------

import { type CSSProperties, type JSX } from 'react';
import { Form, Input, Typography } from 'antd';
import type {
  ProcessDefinition,
  PhaseDefinition,
} from '@/types/process';
import { updatePhaseField } from '../processDefinitionUpdater';

const { Text } = Typography;

export interface PhasePropertyFormProps {
  // 工艺定义（source of truth）
  definition: ProcessDefinition;
  // 当前 phase 的 YAML id
  phaseId: string;
  // 定义变更回调
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
}

export function PhasePropertyForm({
  definition,
  phaseId,
  onDefinitionChange,
}: PhasePropertyFormProps): JSX.Element {
  // 找到当前 phase
  const phase = definition.phases?.find((p) => p.id === phaseId);

  // phase 不存在时显示空状态
  if (!phase) {
    return <Text type="secondary">阶段不存在</Text>;
  }

  // 字段变更通用处理：updatePhaseField → onDefinitionChange
  const handleFieldChange = <K extends keyof PhaseDefinition>(
    field: K,
    value: PhaseDefinition[K],
  ): void => {
    const newDef = updatePhaseField(definition, phaseId, field, value);
    onDefinitionChange(newDef);
  };

  return (
    <Form layout="vertical" style={formStyle}>
      <Form.Item label="标识">
        <Input
          value={phase.id}
          onChange={(e) => handleFieldChange('id', e.target.value)}
        />
      </Form.Item>

      <Form.Item label="名称">
        <Input
          value={phase.name}
          onChange={(e) => handleFieldChange('name', e.target.value)}
        />
      </Form.Item>

      <Form.Item label="规范说明">
        <Input.TextArea
          value={phase.spec ?? ''}
          onChange={(e) => handleFieldChange('spec', e.target.value)}
          rows={3}
          placeholder="阶段规范，可空"
        />
      </Form.Item>

    </Form>
  );
}

// ── 样式 ──────────────────────────────────────────

const formStyle: CSSProperties = {
  padding: 16,
};

