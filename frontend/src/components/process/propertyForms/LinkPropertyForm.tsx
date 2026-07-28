// LinkPropertyForm.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：环节属性面板（LinkPropertyForm）。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.9 + 设计 §7.1）：
// - 暴露 8 个常用字段（id/name/step_template/prompt/executor/review_type/on_success/on_gate_fail）。
// - 嵌套字段：gates + expected_artifacts 用 Ant Design Table 增删行。
// - on_success / on_gate_fail 用分组下拉（OptGroup by phase）。
// - 每个字段 onChange → updateLinkField → onDefinitionChange。
//
// 数据流（M4 单向）：
//   definition → 找到 link → 渲染表单
//   字段修改 → updateLinkField → onDefinitionChange(newDefinition)
// ---------------------------------------------------------------------------

import { type CSSProperties, type JSX } from 'react';
import {
  Form,
  Input,
  Select,
  Button,
  Table,
  Space,
  Typography,
} from 'antd';
import { DeleteOutlined, PlusOutlined } from '@ant-design/icons';
import type {
  ProcessDefinition,
  LinkDefinition,
  GateDefinition,
  ExpectedArtifact,
  StepTemplateRef,
} from '@/types/process';
import { updateLinkField } from '../processDefinitionUpdater';

const { Text } = Typography;

export interface LinkPropertyFormProps {
  // 工艺定义（source of truth）
  definition: ProcessDefinition;
  // 所属 phase 的 YAML id
  phaseId: string;
  // 当前 link 的 YAML id
  linkId: string;
  // 定义变更回调
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
}

export function LinkPropertyForm({
  definition,
  phaseId,
  linkId,
  onDefinitionChange,
}: LinkPropertyFormProps): JSX.Element {
  // 找到当前 link
  const phase = definition.phases?.find((p) => p.id === phaseId);
  const link = phase?.links?.find((l) => l.id === linkId);

  // link 不存在时显示空状态
  if (!link) {
    return <Text type="secondary">环节不存在</Text>;
  }

  // 构建 on_success / on_gate_fail 的分组下拉选项
  // 每个 phase 是一个 OptGroup，组内是该 phase 下的所有 link
  const gotoOptions = buildGotoOptions(definition, linkId);

  // 字段变更通用处理：updateLinkField → onDefinitionChange
  const handleFieldChange = <K extends keyof LinkDefinition>(
    field: K,
    value: LinkDefinition[K],
  ): void => {
    const newDef = updateLinkField(definition, phaseId, linkId, field, value);
    onDefinitionChange(newDef);
  };

  // ── gates 嵌套表格 ─────────────────────────────

  // gates 列定义
  const gatesColumns = [
    {
      title: '名称',
      dataIndex: 'name',
      render: (_: unknown, record: GateDefinition, index: number) => (
        <Input
          value={record.name}
          onChange={(e) =>
            handleGateChange(index, 'name', e.target.value)
          }
          size="small"
        />
      ),
    },
    {
      title: '类型',
      dataIndex: 'type',
      render: (_: unknown, record: GateDefinition, index: number) => (
        <Input
          value={record.type}
          onChange={(e) =>
            handleGateChange(index, 'type', e.target.value)
          }
          size="small"
        />
      ),
    },
    {
      title: '操作',
      render: (_: unknown, __: GateDefinition, index: number) => (
        <Button
          type="text"
          size="small"
          icon={<DeleteOutlined />}
          onClick={() => handleRemoveGate(index)}
        />
      ),
    },
  ];

  // 修改指定 gate 的字段
  const handleGateChange = (
    gateIndex: number,
    field: keyof GateDefinition,
    value: unknown,
  ): void => {
    const newGates = [...(link.gates ?? [])];
    (newGates[gateIndex] as unknown as Record<string, unknown>)[field] = value;
    handleFieldChange('gates', newGates);
  };

  // 新增 gate
  const handleAddGate = (): void => {
    const newGate: GateDefinition = { name: '', type: '' };
    const newGates = [...(link.gates ?? []), newGate];
    handleFieldChange('gates', newGates);
  };

  // 删除 gate
  const handleRemoveGate = (gateIndex: number): void => {
    const newGates = (link.gates ?? []).filter((_, i) => i !== gateIndex);
    handleFieldChange('gates', newGates);
  };

  // ── expected_artifacts 嵌套表格 ─────────────────

  const artifactsColumns = [
    {
      title: '名称',
      dataIndex: 'name',
      render: (_: unknown, record: ExpectedArtifact, index: number) => (
        <Input
          value={record.name}
          onChange={(e) =>
            handleArtifactChange(index, 'name', e.target.value)
          }
          size="small"
        />
      ),
    },
    {
      title: '类型',
      dataIndex: 'type',
      render: (_: unknown, record: ExpectedArtifact, index: number) => (
        <Input
          value={record.type}
          onChange={(e) =>
            handleArtifactChange(index, 'type', e.target.value)
          }
          size="small"
        />
      ),
    },
    {
      title: '操作',
      render: (_: unknown, __: ExpectedArtifact, index: number) => (
        <Button
          type="text"
          size="small"
          icon={<DeleteOutlined />}
          onClick={() => handleRemoveArtifact(index)}
        />
      ),
    },
  ];

  // 修改指定 expected_artifact 的字段
  const handleArtifactChange = (
    artifactIndex: number,
    field: keyof ExpectedArtifact,
    value: unknown,
  ): void => {
    const newArtifacts = [...(link.expected_artifacts ?? [])];
    (newArtifacts[artifactIndex] as unknown as Record<string, unknown>)[field] = value;
    handleFieldChange('expected_artifacts', newArtifacts);
  };

  // 新增 expected_artifact
  const handleAddArtifact = (): void => {
    const newArtifact: ExpectedArtifact = { name: '', type: '' };
    const newArtifacts = [...(link.expected_artifacts ?? []), newArtifact];
    handleFieldChange('expected_artifacts', newArtifacts);
  };

  // 删除 expected_artifact
  const handleRemoveArtifact = (artifactIndex: number): void => {
    const newArtifacts = (link.expected_artifacts ?? []).filter(
      (_, i) => i !== artifactIndex,
    );
    handleFieldChange('expected_artifacts', newArtifacts);
  };

  // ── step_template 嵌套表格（spec 模板引用）─────────

  // step_template 列定义：名称 / 地址 / 操作（删除），与 gates/artifacts 同构。
  const stepTemplateColumns = [
    {
      title: '名称',
      dataIndex: 'name',
      render: (_: unknown, record: StepTemplateRef, index: number) => (
        <Input
          value={record.name}
          onChange={(e) =>
            handleStepTemplateChange(index, 'name', e.target.value)
          }
          size="small"
        />
      ),
    },
    {
      title: '地址',
      dataIndex: 'path',
      render: (_: unknown, record: StepTemplateRef, index: number) => (
        <Input
          value={record.path}
          onChange={(e) =>
            handleStepTemplateChange(index, 'path', e.target.value)
          }
          size="small"
        />
      ),
    },
    {
      title: '操作',
      render: (_: unknown, __: StepTemplateRef, index: number) => (
        <Button
          type="text"
          size="small"
          icon={<DeleteOutlined />}
          onClick={() => handleRemoveStepTemplate(index)}
        />
      ),
    },
  ];

  // 修改指定 step_template 的字段
  const handleStepTemplateChange = (
    idx: number,
    field: keyof StepTemplateRef,
    value: string,
  ): void => {
    const newList = [...(link.step_template ?? [])];
    (newList[idx] as unknown as Record<string, unknown>)[field] = value;
    handleFieldChange('step_template', newList);
  };

  // 新增 step_template（空行，用户填 name/path）
  const handleAddStepTemplate = (): void => {
    const newItem: StepTemplateRef = { name: '', path: '' };
    handleFieldChange('step_template', [
      ...(link.step_template ?? []),
      newItem,
    ]);
  };

  // 删除 step_template
  const handleRemoveStepTemplate = (idx: number): void => {
    handleFieldChange(
      'step_template',
      (link.step_template ?? []).filter((_, i) => i !== idx),
    );
  };

  return (
    <Form layout="vertical" style={formStyle}>
      <Text strong style={sectionTitleStyle}>环节属性</Text>

      <Form.Item label="标识">
        <Input
          value={link.id}
          onChange={(e) => handleFieldChange('id', e.target.value)}
        />
      </Form.Item>

      <Form.Item label="名称">
        <Input
          value={link.name}
          onChange={(e) => handleFieldChange('name', e.target.value)}
        />
      </Form.Item>

      <Form.Item label="提示词">
        <Input.TextArea
          value={link.prompt ?? ''}
          onChange={(e) => handleFieldChange('prompt', e.target.value)}
          rows={3}
          placeholder="提示词，可空"
        />
      </Form.Item>

      <Form.Item label="执行器">
        <Input
          value={link.executor ?? ''}
          onChange={(e) => handleFieldChange('executor', e.target.value)}
          placeholder="执行器，可空"
        />
      </Form.Item>

      <Form.Item label="审核类型">
        <Select
          value={link.review_type ?? 'ai'}
          onChange={(value) => handleFieldChange('review_type', value)}
          options={[
            { value: 'ai', label: 'AI 审核' },
            { value: 'human', label: '人工审核' },
          ]}
        />
      </Form.Item>

      <Form.Item label="成功后跳转">
        <Select
          value={link.on_success ?? 'next'}
          onChange={(value) => handleFieldChange('on_success', value)}
          options={[
            { value: 'next', label: '下一环节' },
            { value: 'end', label: '结束' },
            ...gotoOptions,
          ]}
        />
      </Form.Item>

      <Form.Item label="门禁失败后">
        <Select
          value={link.on_gate_fail ?? 'break'}
          onChange={(value) => handleFieldChange('on_gate_fail', value)}
          options={[
            { value: 'break', label: '中断' },
            ...gotoOptions,
          ]}
        />
      </Form.Item>

      <Form.Item label="验收标准">
        <Input.TextArea
          value={link.acceptance_criteria ?? ''}
          onChange={(e) =>
            handleFieldChange('acceptance_criteria', e.target.value)
          }
          rows={2}
          placeholder="环节产物验收标准，可空"
        />
      </Form.Item>

      {/* gates 嵌套表格 */}
      <Space style={sectionHeaderStyle}>
        <Text strong>门禁</Text>
        <Button
          size="small"
          icon={<PlusOutlined />}
          onClick={handleAddGate}
        >
          新增门禁
        </Button>
      </Space>
      <Table
        dataSource={link.gates ?? []}
        columns={gatesColumns}
        rowKey={(_, index) => String(index)}
        pagination={false}
        size="small"
        style={tableStyle}
      />

      {/* expected_artifacts 嵌套表格 */}
      <Space style={sectionHeaderStyle}>
        <Text strong>期望产物</Text>
        <Button
          size="small"
          icon={<PlusOutlined />}
          onClick={handleAddArtifact}
        >
          新增产物
        </Button>
      </Space>
      <Table
        dataSource={link.expected_artifacts ?? []}
        columns={artifactsColumns}
        rowKey={(_, index) => String(index)}
        pagination={false}
        size="small"
        style={tableStyle}
      />

      {/* step_template 嵌套表格（spec 模板引用） */}
      <Space style={sectionHeaderStyle}>
        <Text strong>环节 spec 模板</Text>
        <Button
          size="small"
          icon={<PlusOutlined />}
          onClick={handleAddStepTemplate}
        >
          新增模板
        </Button>
      </Space>
      <Table
        dataSource={link.step_template ?? []}
        columns={stepTemplateColumns}
        rowKey={(_, index) => String(index)}
        pagination={false}
        size="small"
        style={tableStyle}
      />
    </Form>
  );
}

// ── 辅助函数 ──────────────────────────────────────

// 构建 on_success / on_gate_fail 的分组下拉选项。
// 每个 phase 是一个 OptGroup，组内是该 phase 下的所有 link。
// 排除当前 link 自身（不能 goto 自己）。
function buildGotoOptions(
  definition: ProcessDefinition,
  currentLinkId: string,
): Array<{ label: string; options: Array<{ value: string; label: string }> }> {
  const result: Array<{
    label: string;
    options: Array<{ value: string; label: string }>;
  }> = [];

  for (const phase of definition.phases ?? []) {
    const options: Array<{ value: string; label: string }> = [];
    for (const link of phase.links ?? []) {
      // 排除当前 link 自身
      if (link.id === currentLinkId) continue;
      options.push({
        value: `goto:${link.id}`,
        label: `${link.name} (${link.id})`,
      });
    }
    // 只在有选项时添加该 phase 的 OptGroup
    if (options.length > 0) {
      result.push({ label: phase.name, options });
    }
  }

  return result;
}

// ── 样式 ──────────────────────────────────────────

const formStyle: CSSProperties = {
  padding: 16,
};

const sectionTitleStyle: CSSProperties = {
  display: 'block',
  marginBottom: 16,
};

const sectionHeaderStyle: CSSProperties = {
  width: '100%',
  justifyContent: 'space-between',
  marginBottom: 8,
  marginTop: 16,
};

const tableStyle: CSSProperties = {
  marginBottom: 16,
};
