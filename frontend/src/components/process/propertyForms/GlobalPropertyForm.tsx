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
  Tooltip,
  Typography,
} from 'antd';
import type { ProcessDefinition } from '@/types/process';

const { Text } = Typography;

// 异常处理 prompt 运行时替换的模板参数（需求 035）。
// 双花括号占位符，与评审 prompt 一致；后端 compose_abnormal_handler_prompt 只替换双花括号。
const ABNORMAL_HANDLER_PROMPT_PARAMS = [
  { key: '{{loop_name}}', desc: 'Loop 名称' },
  { key: '{{loop_execution_id}}', desc: '本次 Loop 执行 ID' },
  { key: '{{abnormal_status}}', desc: '异常状态（capped_step/capped_token/failed/partial）' },
  { key: '{{total_executed_steps}}', desc: '已执行步数' },
  { key: '{{total_tokens_used}}', desc: '已消耗 Token 数' },
  { key: '{{error_detail}}', desc: '失败原因 / 错误信息' },
];

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
  const abnormalHandler = definition.abnormal_handler ?? {};
  const triggerOn = Array.isArray(abnormalHandler.trigger_on)
    ? abnormalHandler.trigger_on
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

  // 修改 abnormal_handler.prompt（异常处理提示词）
  const handleAbnormalPromptChange = (value: string): void => {
    const newDef: ProcessDefinition = {
      ...definition,
      abnormal_handler: { ...abnormalHandler, prompt: value },
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
                <Form.Item label="名称">
                  <Input
                    value={meta.name}
                    onChange={(e) => handleMetaChange('name', e.target.value)}
                  />
                </Form.Item>
                <Form.Item label="显示名称">
                  <Input
                    value={meta.display_name ?? ''}
                    onChange={(e) =>
                      handleMetaChange('display_name', e.target.value)
                    }
                  />
                </Form.Item>
                <Form.Item label="分类">
                  <Input
                    value={meta.category ?? ''}
                    onChange={(e) =>
                      handleMetaChange('category', e.target.value)
                    }
                  />
                </Form.Item>
                <Form.Item label="复杂度">
                  <Select
                    value={meta.complexity ?? 'light'}
                    onChange={(value) =>
                      handleMetaChange('complexity', value)
                    }
                    options={[
                      { value: 'light', label: '轻量' },
                      { value: 'standard', label: '标准' },
                      { value: 'complex', label: '复杂' },
                    ]}
                  />
                </Form.Item>
                <Form.Item label="版本">
                  <Input
                    value={meta.version ?? '1.0.0'}
                    onChange={(e) =>
                      handleMetaChange('version', e.target.value)
                    }
                  />
                </Form.Item>
                <Form.Item label="描述">
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
      <Text strong style={sectionTitleStyle}>全局限制</Text>
      <Form.Item label="单环节最大执行数">
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
      <Form.Item label="全局最大 Token 数">
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
      <Text strong style={sectionTitleStyle}>异常处理</Text>
      <Form.Item label="触发条件">
        <Checkbox.Group
          value={triggerOn}
          onChange={(values) => handleTriggerOnChange(values as string[])}
          options={[
            { value: 'capped_step', label: '步数超限' },
            { value: 'capped_token', label: 'Token 超限' },
            { value: 'failed', label: '失败' },
          ]}
        />
      </Form.Item>
      <Form.Item
        label="异常处理 Prompt"
        tooltip="环路异常终止（超步数/超Token/失败）时执行此提示词。可用下方参数占位符"
      >
        <Input.TextArea
          value={abnormalHandler.prompt ?? ''}
          onChange={(e) => handleAbnormalPromptChange(e.target.value)}
          rows={4}
          placeholder="异常发生时执行的补救/清理提示词，可空。可用 {{abnormal_status}}、{{error_detail}} 等占位符"
        />
        {/* 快速插入参数条：点击把 {{key}} 追加到异常处理 prompt 尾部，与评审 prompt 参数条样式一致 */}
        <div style={{
          marginTop: 8,
          display: 'flex',
          flexWrap: 'wrap',
          gap: 6,
          alignItems: 'center',
        }}>
          <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)', marginRight: 2 }}>可用参数:</span>
          {ABNORMAL_HANDLER_PROMPT_PARAMS.map((p) => (
            <Tooltip key={p.key} title={p.desc}>
              <code
                onClick={() => handleAbnormalPromptChange(
                  (abnormalHandler.prompt ?? '') + (abnormalHandler.prompt?.endsWith(' ') || !abnormalHandler.prompt ? '' : ' ') + p.key + ' ',
                )}
                style={{
                  fontSize: 11,
                  padding: '1px 6px',
                  borderRadius: 4,
                  background: 'var(--color-fill-quaternary)',
                  border: '1px solid var(--color-border-secondary)',
                  cursor: 'pointer',
                  color: 'var(--color-text-secondary)',
                  transition: 'all 0.2s',
                }}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-primary)';
                  (e.currentTarget as HTMLElement).style.color = 'var(--color-primary)';
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-border-secondary)';
                  (e.currentTarget as HTMLElement).style.color = 'var(--color-text-secondary)';
                }}
              >
                {p.key}
              </code>
            </Tooltip>
          ))}
        </div>
      </Form.Item>


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
