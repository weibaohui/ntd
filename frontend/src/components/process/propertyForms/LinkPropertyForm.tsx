// LinkPropertyForm.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：环节属性面板（LinkPropertyForm）。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.9 + 设计 §7.1）：
// - 暴露 7 个常用字段（id/name/step_template/prompt/executor/on_success/on_gate_fail）。
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
  InputNumber,
  Select,
  Button,
  Table,
  Space,
  Typography,
} from 'antd';
import { DeleteOutlined, PlusOutlined, CloseOutlined } from '@ant-design/icons';
import { DefaultReviewPromptButton } from '@/components/common/DefaultReviewPromptButton';
// 需求 046：prompt 类字段统一改用 MD 编辑控件（含参数条的光标插入）
import { PromptMdField } from './PromptMdField';
import type {
  ProcessDefinition,
  LinkDefinition,
  GateDefinition,
  ExpectedArtifact,
  StepTemplateRef,
} from '@/types/process';
import { updateLinkField } from '../processDefinitionUpdater';
// 专家/技能复用 todo 侧组件：ExpertPicker 受控选择；LinkSkillPicker 封装技能加载 + toggle
import { ExpertPicker } from '@/components/todo-drawer/ExpertPicker';
import { LinkSkillPicker } from './LinkSkillPicker';
// 评审 prompt 运行时替换的模板参数，用于 AI 评审时的 prompt 合成。
// {{original_prompt}} → 当前环节的提示词
// {{original_output}} → 执行记录的输出结果
// {{acceptance_criteria}} → 验收标准
// 占位符统一用 {{双花括号}}：后端 compose_review_prompt / apply_rating_gate 只替换双花括号，
// 单花括号不会被替换。历史上曾混用 {单括号} 导致占位符原样发给 AI，已统一为双花括号。
const REVIEW_PROMPT_PARAMS = [
  { key: '{{original_prompt}}', label: 'original_prompt', desc: '当前环节的提示词（prompt 字段）' },
  { key: '{{original_output}}', label: 'original_output', desc: '执行记录的输出结果' },
  { key: '{{acceptance_criteria}}', label: 'acceptance_criteria', desc: '验收标准，用于评分评价' },
];

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

  // on_success / on_gate_fail 的分组下拉选项。
  // - gotoOptionsNoSelf：排除当前环节（on_success 成功后重跑自己无意义）。
  // - gotoOptionsWithSelf：含当前环节（on_gate_fail 可 goto 自己 = 失败重试，由 max_rework 限制次数）。
  const gotoOptionsNoSelf = buildGotoOptions(definition, linkId, false);
  const gotoOptionsWithSelf = buildGotoOptions(definition, linkId, true);

  // 字段变更通用处理：updateLinkField → onDefinitionChange
  const handleFieldChange = <K extends keyof LinkDefinition>(
    field: K,
    value: LinkDefinition[K],
  ): void => {
    const newDef = updateLinkField(definition, phaseId, linkId, field, value);
    onDefinitionChange(newDef);
  };

  // 技能 toggle：点击技能卡片加入/移出 link.skills[]（环节级技能，运行时用）
  const handleToggleSkill = (skillName: string): void => {
    const current = link.skills ?? [];
    const next = current.includes(skillName)
      ? current.filter((s) => s !== skillName)
      : [...current, skillName];
    handleFieldChange('skills', next);
  };

  // ── gates 嵌套表格 ─────────────────────────────

  // 按门禁类型渲染对应配置字段：
  // artifact_present→产物名、ai_criteria_review→及格分、script_check→脚本路径、human_approval→无需配置。
  // 字段对应后端 GateDefinition 的 artifact/min_score/script，criteria_ref 暂不暴露（用验收标准字段替代）。
  const renderGateConfig = (
    record: GateDefinition,
    index: number,
  ): JSX.Element => {
    switch (record.type) {
      case 'artifact_present':
        return (
          <Input
            value={record.artifact ?? ''}
            onChange={(e) =>
              handleGateChange(index, 'artifact', e.target.value)
            }
            size="small"
            placeholder="产物名，如 PRD"
          />
        );
      case 'ai_criteria_review':
        return (
          <InputNumber
            value={record.min_score}
            onChange={(value) =>
              handleGateChange(
                index,
                'min_score',
                value === null ? undefined : value,
              )
            }
            min={0}
            max={100}
            size="small"
            style={{ width: '100%' }}
            placeholder="及格分，如 80"
          />
        );
      case 'script_check':
        return (
          <Input
            value={record.script ?? ''}
            onChange={(e) =>
              handleGateChange(index, 'script', e.target.value)
            }
            size="small"
            placeholder="脚本路径，如 run_tests.sh"
          />
        );
      case 'human_approval':
        return <Text type="secondary">无需配置</Text>;
      default:
        return <Text type="secondary">选择类型后配置</Text>;
    }
  };

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
        <Select
          value={record.type || undefined}
          onChange={(value) => handleGateChange(index, 'type', value)}
          options={GATE_TYPE_OPTIONS}
          size="small"
          style={{ width: '100%' }}
          placeholder="选择门禁类型"
        />
      ),
    },
    {
      // 配置列：按门禁类型动态显示对应字段（artifact/min_score/script）
      title: '配置',
      render: (_: unknown, record: GateDefinition, index: number) =>
        renderGateConfig(record, index),
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

      {/* 提示词：需求 046 由 TextArea 升级为 MD 编辑器（200px）。
          不传 params：该字段现状无参数条，保持能力不扩张（YAGNI） */}
      <Form.Item label="提示词">
        <PromptMdField
          value={link.prompt ?? ''}
          onChange={(v) => handleFieldChange('prompt', v)}
        />
      </Form.Item>

      {/* 验收标准：环节产物验收标准，评审打分依据（需求 037 移到提示词后） */}
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

      {/* 专家：复用 todo 的 ExpertPicker，存 link.expert（组件自带清除按钮） */}
      <Form.Item label="专家">
        <ExpertPicker
          value={link.expert}
          onChange={(v) => handleFieldChange('expert', v ?? undefined)}
        />
      </Form.Item>

      {/* 技能：按环节 executor 加载，复用 SkillSelector 卡片，点击 toggle 存 link.skills[] */}
      <LinkSkillPicker
        executor={link.executor}
        selectedSkills={link.skills}
        onToggleSkill={handleToggleSkill}
      />

      {/* 048：审核类型(review_type)已废弃——评审/门禁统一由下方 gates 配置
          (ai_criteria_review / human_approval) 决定，不再单列 review_type 字段。 */}

      {/* 评审 Prompt：需求 046 升级为 MD 编辑器（200px）。
          原 TextArea placeholder 中的使用说明迁移到 Form.Item tooltip，避免提示信息丢失；
          参数插入条迁入 PromptMdField，插入行为由尾部追加升级为光标处插入（对齐 todo 页）。 */}
      <Form.Item
        label="评审 Prompt"
        tooltip="环节专属评审模板，可空。非空时整体替代默认评审模板；可用下方参数占位符"
      >
        <PromptMdField
          value={link.review_prompt ?? ''}
          onChange={(v) => handleFieldChange('review_prompt', v)}
          params={REVIEW_PROMPT_PARAMS}
          extraActions={
            /* 一键填入系统默认评审 prompt（DEFAULT_REVIEWER_PROMPT 常量），直接覆盖原内容 */
            <DefaultReviewPromptButton onApply={(t) => handleFieldChange('review_prompt', t)} />
          }
        />
      </Form.Item>

      <Form.Item label="成功后跳转">
        <Select
          value={link.on_success}
          onChange={(value) => handleFieldChange('on_success', value ?? undefined)}
          allowClear
          placeholder="默认：下一环节"
          options={[
            { value: 'next', label: '下一环节' },
            { value: 'end', label: '结束' },
            ...gotoOptionsNoSelf,
          ]}
        />
      </Form.Item>

      <Form.Item label="门禁失败后">
        <Select
          value={link.on_gate_fail}
          onChange={(value) => handleFieldChange('on_gate_fail', value ?? undefined)}
          allowClear
          placeholder="默认：中断"
          options={[
            { value: 'break', label: '中断' },
            ...gotoOptionsWithSelf,
          ]}
        />
      </Form.Item>

      {/* 最大返工次数：门禁失败跳转返工的上限，超过即终止工艺（防止死循环重试）。
          对应 loop_steps.max_rework，后端默认 3。空 = 用默认值。 */}
      <Form.Item label="最大返工次数" tooltip="门禁失败跳转返工的上限次数，超过即终止工艺。留空用默认 3。">
        <InputNumber
          value={link.max_rework}
          onChange={(value) =>
            handleFieldChange('max_rework', value === null ? undefined : value)
          }
          min={0}
          max={20}
          style={{ width: '100%' }}
          placeholder="默认：3"
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

      {/* 执行器：需求 037 移到面板最后；suffix 清除按钮满足"可清空" */}
      <Form.Item label="执行器">
        <Input
          value={link.executor ?? ''}
          onChange={(e) => handleFieldChange('executor', e.target.value)}
          placeholder="执行器，可空"
          suffix={
            link.executor ? (
              <CloseOutlined
                onClick={() => handleFieldChange('executor', undefined)}
                style={{ color: 'var(--color-text-tertiary)', cursor: 'pointer' }}
              />
            ) : null
          }
        />
      </Form.Item>
    </Form>
  );
}

// ── 辅助函数 ──────────────────────────────────────

// 构建 on_success / on_gate_fail 的分组下拉选项。
// 每个 phase 是一个 OptGroup，组内是该 phase 下的所有 link。
// includeSelf=false 排除当前环节（on_success：成功后重跑自己无意义）；
// includeSelf=true 含当前环节（on_gate_fail：goto 自己 = 失败重试，由 max_rework 限制次数防死循环）。
function buildGotoOptions(
  definition: ProcessDefinition,
  currentLinkId: string,
  includeSelf: boolean,
): Array<{ label: string; options: Array<{ value: string; label: string }> }> {
  const result: Array<{
    label: string;
    options: Array<{ value: string; label: string }>;
  }> = [];

  for (const phase of definition.phases ?? []) {
    const options: Array<{ value: string; label: string }> = [];
    for (const link of phase.links ?? []) {
      // includeSelf=false 时排除当前环节自身
      if (!includeSelf && link.id === currentLinkId) continue;
      options.push({
        value: link.id,
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

const sectionHeaderStyle: CSSProperties = {
  width: '100%',
  justifyContent: 'space-between',
  marginBottom: 8,
  marginTop: 16,
};

const tableStyle: CSSProperties = {
  marginBottom: 16,
};

// 门禁类型可选项。046 起 artifact_present/script_check 已废弃（统一并入 ai_criteria_review），
// 编辑器不再产出这两种类型，避免建出运行时报「废弃门禁类型」错误的工艺。
// 用下拉避免用户手填拼错 type 字符串，导致门禁匹配不到执行器而不生效。
const GATE_TYPE_OPTIONS = [
  { value: 'ai_criteria_review', label: 'AI 评审' },
  { value: 'human_approval', label: '人工审批' },
];
