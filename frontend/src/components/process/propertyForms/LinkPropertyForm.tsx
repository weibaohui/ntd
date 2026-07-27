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
      title: 'name',
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
      title: 'type',
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
      title: 'name',
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
      title: 'type',
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

  return (
    <Form layout="vertical" style={formStyle}>
      <Text strong style={sectionTitleStyle}>环节属性</Text>

      <Form.Item label="id">
        <Input
          value={link.id}
          onChange={(e) => handleFieldChange('id', e.target.value)}
        />
      </Form.Item>

      <Form.Item label="name">
        <Input
          value={link.name}
          onChange={(e) => handleFieldChange('name', e.target.value)}
        />
      </Form.Item>

      <Form.Item label="step_template">
        <Input
          value={link.step_template ?? ''}
          onChange={(e) => handleFieldChange('step_template', e.target.value)}
          placeholder="原型引用，可空"
        />
      </Form.Item>

      <Form.Item label="prompt">
        <Input.TextArea
          value={link.prompt ?? ''}
          onChange={(e) => handleFieldChange('prompt', e.target.value)}
          rows={3}
          placeholder="提示词，可空"
        />
      </Form.Item>

      <Form.Item label="executor">
        <Input
          value={link.executor ?? ''}
          onChange={(e) => handleFieldChange('executor', e.target.value)}
          placeholder="执行器，可空"
        />
      </Form.Item>

      <Form.Item label="review_type">
        <Select
          value={link.review_type ?? 'ai'}
          onChange={(value) => handleFieldChange('review_type', value)}
          options={[
            { value: 'ai', label: 'ai' },
            { value: 'human', label: 'human' },
          ]}
        />
      </Form.Item>

      <Form.Item label="on_success">
        <Select
          value={link.on_success ?? 'next'}
          onChange={(value) => handleFieldChange('on_success', value)}
          options={[
            { value: 'next', label: 'next' },
            { value: 'end', label: 'end' },
            ...gotoOptions,
          ]}
        />
      </Form.Item>

      <Form.Item label="on_gate_fail">
        <Select
          value={link.on_gate_fail ?? 'break'}
          onChange={(value) => handleFieldChange('on_gate_fail', value)}
          options={[
            { value: 'break', label: 'break' },
            ...gotoOptions,
          ]}
        />
      </Form.Item>

      {/* gates 嵌套表格 */}
      <Space style={sectionHeaderStyle}>
        <Text strong>gates（门禁）</Text>
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
        <Text strong>expected_artifacts（期望产物）</Text>
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
