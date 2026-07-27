// GlobalPropertyForm.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：全局面板（GlobalPropertyForm）。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.11 + 设计 §7.3）：
// - 顶部折叠面板：工艺元信息（name/display_name/category/complexity/version/description）
// - limits 小表单：max_step_executions + max_total_tokens
// - abnormal_handler 静态表单：trigger_on 多选
// - step_templates Collapse 折叠面板
//
// 数据流（M4 单向）：
//   definition → 渲染表单
//   字段修改 → 浅克隆 definition 并赋值 → onDefinitionChange(newDefinition)
// ---------------------------------------------------------------------------

import { type CSSProperties, type JSX } from 'react';
import {
  Form,
  Input,
  Select,
  InputNumber,
  Checkbox,
  Collapse,
  Typography,
} from 'antd';
import type { ProcessDefinition } from '@/types/process';

const { Text } = Typography;

export interface GlobalPropertyFormProps {
  // 工艺定义（source of truth）
  definition: ProcessDefinition;
  // 定义变更回调
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
}

export function GlobalPropertyForm({
  definition,
  onDefinitionChange,
}: GlobalPropertyFormProps): JSX.Element {
  // 工艺元信息（definition.process）
  const meta = definition.process;

  // 全局限制（definition.limits）
  const limits = definition.limits ?? {
    max_step_executions: undefined,
    max_total_tokens: undefined,
  };

  // 异常处理（definition.abnormal_handler）
  // abnormal_handler 是 unknown 类型，我们用宽松处理
  const abnormalHandler = (definition.abnormal_handler ?? {}) as Record<
    string,
    unknown
  >;
  const triggerOn = Array.isArray(abnormalHandler.trigger_on)
    ? (abnormalHandler.trigger_on as string[])
    : [];

  // ── 字段变更处理 ─────────────────────────────────

  // 修改 process 元信息字段
  const handleMetaChange = (
    field: keyof typeof meta,
    value: unknown,
  ): void => {
    const newDef: ProcessDefinition = {
      ...definition,
      process: { ...meta, [field]: value },
    };
    onDefinitionChange(newDef);
  };

  // 修改 limits 字段
  const handleLimitsChange = (
    field: 'max_step_executions' | 'max_total_tokens',
    value: number | undefined,
  ): void => {
    const newDef: ProcessDefinition = {
      ...definition,
      limits: { ...limits, [field]: value },
    };
    onDefinitionChange(newDef);
  };

  // 修改 abnormal_handler.trigger_on
  const handleTriggerOnChange = (values: string[]): void => {
    const newDef: ProcessDefinition = {
      ...definition,
      abnormal_handler: { ...abnormalHandler, trigger_on: values },
    };
    onDefinitionChange(newDef);
  };

  return (
    <Form layout="vertical" style={formStyle}>
      {/* 工艺元信息（顶部折叠面板） */}
      <Collapse
        defaultActiveKey={['meta']}
        style={collapseStyle}
        items={[
          {
            key: 'meta',
            label: '工艺元信息',
            children: (
              <>
                <Form.Item label="name">
                  <Input
                    value={meta.name}
                    onChange={(e) => handleMetaChange('name', e.target.value)}
                  />
                </Form.Item>
                <Form.Item label="display_name">
                  <Input
                    value={meta.display_name ?? ''}
                    onChange={(e) =>
                      handleMetaChange('display_name', e.target.value)
                    }
                  />
                </Form.Item>
                <Form.Item label="category">
                  <Input
                    value={meta.category ?? ''}
                    onChange={(e) =>
                      handleMetaChange('category', e.target.value)
                    }
                  />
                </Form.Item>
                <Form.Item label="complexity">
                  <Select
                    value={meta.complexity ?? 'light'}
                    onChange={(value) =>
                      handleMetaChange('complexity', value)
                    }
                    options={[
                      { value: 'light', label: 'light' },
                      { value: 'standard', label: 'standard' },
                      { value: 'complex', label: 'complex' },
                    ]}
                  />
                </Form.Item>
                <Form.Item label="version">
                  <Input
                    value={meta.version ?? '1.0.0'}
                    onChange={(e) =>
                      handleMetaChange('version', e.target.value)
                    }
                  />
                </Form.Item>
                <Form.Item label="description">
                  <Input.TextArea
                    value={meta.description ?? ''}
                    onChange={(e) =>
                      handleMetaChange('description', e.target.value)
                    }
                    rows={2}
                  />
                </Form.Item>
              </>
            ),
          },
        ]}
      />

      {/* limits 小表单 */}
      <Text strong style={sectionTitleStyle}>limits（全局限制）</Text>
      <Form.Item label="max_step_executions">
        <InputNumber
          value={limits.max_step_executions}
          onChange={(value) =>
            handleLimitsChange(
              'max_step_executions',
              value === null ? undefined : value,
            )
          }
          min={1}
          style={inputNumberStyle}
        />
      </Form.Item>
      <Form.Item label="max_total_tokens">
        <InputNumber
          value={limits.max_total_tokens}
          onChange={(value) =>
            handleLimitsChange(
              'max_total_tokens',
              value === null ? undefined : value,
            )
          }
          min={1}
          style={inputNumberStyle}
        />
      </Form.Item>

      {/* abnormal_handler 静态表单 */}
      <Text strong style={sectionTitleStyle}>abnormal_handler（异常处理）</Text>
      <Form.Item label="trigger_on">
        <Checkbox.Group
          value={triggerOn}
          onChange={(values) => handleTriggerOnChange(values as string[])}
          options={[
            { value: 'capped_step', label: 'capped_step' },
            { value: 'capped_token', label: 'capped_token' },
            { value: 'failed', label: 'failed' },
          ]}
        />
      </Form.Item>

      {/* step_templates Collapse 折叠面板 */}
      <Text strong style={sectionTitleStyle}>step_templates（环节原型）</Text>
      <Collapse
        style={collapseStyle}
        items={[
          {
            key: 'templates',
            label: '环节原型列表',
            children: (
              <Text type="secondary">
                step_templates 编辑将在 M5 完善（当前展示元信息）
              </Text>
            ),
          },
        ]}
      />
    </Form>
  );
}

// ── 样式 ──────────────────────────────────────────

const formStyle: CSSProperties = {
  padding: 16,
};

const sectionTitleStyle: CSSProperties = {
  display: 'block',
  marginBottom: 16,
  marginTop: 24,
};

const collapseStyle: CSSProperties = {
  marginBottom: 16,
};

const inputNumberStyle: CSSProperties = {
  width: '100%',
};
