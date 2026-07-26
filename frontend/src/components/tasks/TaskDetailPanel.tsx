// 嵌入式任务详情面板。
// 与 TasksPage 列表态的 ListDetailPage 双栏右栏配套使用。
// 不含返回按钮（由宿主组件控制），由宿主在选中任务时挂载、取消选中时卸载。

import { useEffect, useState } from 'react';
import { Card, List, Tag, Button, Typography, Spin, Collapse, Space, message, Descriptions, Modal, Input, Empty } from 'antd';
import { ThunderboltOutlined, CaretRightOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import { ProcessExecutionBoard } from '@/components/process/ProcessExecutionBoard';
import { complexityColor, complexityLabel, statusColor, formatDateShort } from './constants';

const { Title, Text } = Typography;

interface TaskDetailPanelProps {
  taskId: number;
  workspaceId: number;
  /** 任何变更（再次执行）后回调，让宿主重拉列表保持口径一致。 */
  onTriggered?: () => void;
}

interface StepInfo {
  id: number;
  name: string;
  order_index: number;
  skill_names: string[];
  expected_artifacts: Array<{ name: string; path?: string; locator?: string; type: string }>;
  gate_config: Array<{ name: string; type: string }>;
}

interface ExecInfo {
  id: number;
  status: string;
  started_at?: string;
  finished_at?: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
  requirement?: string;
}

interface TaskDetailData {
  task: { id: number; title: string; status: string; description?: string; workspace_id?: number; loop_id?: number };
  template?: { display_name?: string; version?: string; complexity?: string };
  steps: StepInfo[];
  executions: ExecInfo[];
  loop?: { id: number; workspace_id?: number };
}

/** 门禁类型 → 中文标签。 */
function gateLabel(type: string): string {
  const map: Record<string, string> = {
    artifact_present: '产物存在',
    ai_criteria_review: 'AI 评审',
    human_approval: '人工审批',
    script_check: '脚本校验',
  };
  return map[type] ?? type;
}

/** 单条工艺步骤卡片，逐项展示技能/产物/门禁。 */
function StepCard({ step }: { step: StepInfo }) {
  return (
    <Card key={step.id} size="small" style={{ marginBottom: 8 }}>
      <Space direction="vertical" size={4} style={{ width: '100%' }}>
        {/* 标题行：序号 + 步骤名 */}
        <Space>
          <Text type="secondary">{step.order_index + 1}.</Text>
          <Text strong>{step.name}</Text>
        </Space>
        {/* 技能标签行：仅在 step.skill_names 非空时渲染 */}
        {step.skill_names.length > 0 && (
          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
            <Text type="secondary" style={{ fontSize: 12 }}>技能：</Text>
            {step.skill_names.map((sk) => (
              <Tag key={sk} color="purple" style={{ fontSize: 12 }}>{sk}</Tag>
            ))}
          </div>
        )}
        {/* 产物标签行：未列出时跳过 */}
        {step.expected_artifacts.length > 0 && (
          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
            <Text type="secondary" style={{ fontSize: 12 }}>产物：</Text>
            {step.expected_artifacts.map((a, i) => (
              <Tag key={i} color="blue" style={{ fontSize: 12 }}>
                {a.name} → {a.path ?? a.locator} ({a.type})
              </Tag>
            ))}
          </div>
        )}
        {/* 门禁标签行：未列出时跳过 */}
        {step.gate_config.length > 0 && (
          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
            <Text type="secondary" style={{ fontSize: 12 }}>门禁：</Text>
            {step.gate_config.map((g, i) => (
              <Tag key={i} style={{ fontSize: 12 }}>{g.name} ({gateLabel(g.type)})</Tag>
            ))}
          </div>
        )}
      </Space>
    </Card>
  );
}

/**
 * 任务详情面板。
 *
 * 整体处理思路：
 * 1. 通过 bundledApi.getTaskDetail 一次性拉取任务/模板/步骤/执行历史。
 * 2. 头部用 Descriptions 展示模板/版本/复杂度/状态概览。
 * 3. 工艺要求用 Collapse 默认展开，每步用 StepCard 渲染。
 * 4. 执行历史用 List + 「查看详情」展开 ProcessExecutionBoard。
 * 5. 「再次执行」按钮打开 Modal，调 createTaskExecution 创建新执行。
 */
export function TaskDetailPanel({ taskId, workspaceId, onTriggered }: TaskDetailPanelProps) {
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<TaskDetailData | null>(null);
  const [activeExec, setActiveExec] = useState<number | null>(null);
  const [triggering, setTriggering] = useState(false);
  const [reqModalOpen, setReqModalOpen] = useState(false);
  const [newRequirement, setNewRequirement] = useState('');

  const load = async () => {
    setLoading(true);
    try {
      // 直接拿 any 然后类型断言：与现有 TaskDetailPage 一致，
      // bundledApi.getTaskDetail 返回 any，避免引入复杂类型。
      const raw = (await bundledApi.getTaskDetail(workspaceId, taskId)) as TaskDetailData;
      setDetail(raw);
    } catch {
      message.error('加载失败');
    } finally {
      setLoading(false);
    }
  };

  // taskId/workspaceId 变化时重拉；卸载时若仍在 loading 不影响已设置状态。
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId, workspaceId]);

  const handleNewExec = async () => {
    if (!newRequirement.trim()) {
      message.warning('请输入需求');
      return;
    }
    setTriggering(true);
    try {
      await bundledApi.createTaskExecution(workspaceId, taskId, newRequirement);
      message.success('新执行已创建');
      setReqModalOpen(false);
      setNewRequirement('');
      load();
      onTriggered?.();
    } catch {
      message.error('创建失败');
    } finally {
      setTriggering(false);
    }
  };

  if (loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;
  if (!detail) return <Empty description="暂无任务详情" style={{ marginTop: 48 }} />;

  const { task, template, steps, executions } = detail;
  const wsId = task.workspace_id ?? detail.loop?.workspace_id ?? 1;
  const lpId = task.loop_id ?? detail.loop?.id ?? 0;

  return (
    <div style={{ padding: 24, height: '100%', overflow: 'auto' }}>
      {/* 概览卡：模板 / 版本 / 复杂度 / 状态 */}
      <Card style={{ marginBottom: 16 }}>
        <Descriptions
          title={
            <Title level={4} style={{ margin: 0 }}>
              <Text type="secondary">#{task.id}</Text> {task.title}
            </Title>
          }
          size="small"
          column={2}
          items={[
            { label: '模板', children: template?.display_name ?? '—' },
            { label: '版本', children: template?.version ?? '—' },
            {
              label: '复杂度',
              children: template?.complexity
                ? <Tag color={complexityColor(template.complexity)}>{complexityLabel(template.complexity)}</Tag>
                : '—',
            },
            {
              label: '状态',
              children: <Tag color={statusColor(task.status)}>{task.status}</Tag>,
            },
          ]}
        />
      </Card>

      {/* 工艺要求：Collapse 默认展开，每步用 StepCard 渲染 */}
      <Collapse
        defaultActiveKey={['req']}
        style={{ marginBottom: 16 }}
        items={[{
          key: 'req',
          label: <Text strong>工艺要求（{steps?.length ?? 0} 步）</Text>,
          children: (steps ?? []).map((s) => <StepCard key={s.id} step={s} />),
        }]}
      />

      {/* 执行历史：标题 + 再次执行按钮 + List */}
      <div style={{ display: 'flex', alignItems: 'center', marginBottom: 12, marginTop: 16 }}>
        <Title level={4} style={{ margin: 0, marginRight: 8 }}>执行历史</Title>
        <Button
          icon={<ThunderboltOutlined />}
          onClick={() => { setNewRequirement(task.description ?? task.title); setReqModalOpen(true); }}
          size="small"
        >
          再次执行
        </Button>
      </div>
      <List
        dataSource={executions ?? []}
        locale={{ emptyText: <Empty description="暂无执行" image={Empty.PRESENTED_IMAGE_SIMPLE} /> }}
        renderItem={(e) => (
          <List.Item
            actions={[
              <Button
                type="link"
                icon={<CaretRightOutlined />}
                onClick={() => setActiveExec(activeExec === e.id ? null : e.id)}
                key="v"
              >
                {activeExec === e.id ? '收起' : '查看详情'}
              </Button>,
            ]}
          >
            <List.Item.Meta
              title={
                <Space>
                  <Tag color={statusColor(e.status)}>{e.status}</Tag>
                  <Text>#{e.id} {e.completed_steps}/{e.total_steps} 完成</Text>
                  {e.failed_steps > 0 && <Tag color="orange">失败 {e.failed_steps}</Tag>}
                </Space>
              }
              description={e.requirement || <Text type="secondary">{formatDateShort(e.started_at)}</Text>}
            />
          </List.Item>
        )}
      />

      {/* 当前选中执行：嵌入 ProcessExecutionBoard */}
      {activeExec && (
        <div style={{ marginTop: 16 }}>
          <ProcessExecutionBoard
            workspaceId={wsId}
            loopId={lpId}
            executionId={activeExec}
          />
        </div>
      )}

      {/* 再次执行 Modal */}
      <Modal
        title="输入这次的需求"
        open={reqModalOpen}
        onCancel={() => setReqModalOpen(false)}
        onOk={handleNewExec}
        confirmLoading={triggering}
        okText="开始执行"
      >
        <Input.TextArea value={newRequirement} onChange={(e) => setNewRequirement(e.target.value)} rows={4} />
      </Modal>
    </div>
  );
}
