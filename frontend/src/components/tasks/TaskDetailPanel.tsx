// 嵌入式任务详情面板（Tabs 布局重构版）。
// 与 TasksPage 详情态配套使用：顶部标题栏由宿主 PageCard 提供（含返回按钮），
// 本组件自身提供「顶部条（标题+元信息+再次执行）」+「Tabs（概览/工艺要求/执行历史）」。

import { useEffect, useState, type ReactNode } from 'react';
import {
  Tabs,
  Tag,
  Button,
  Typography,
  Spin,
  List,
  Progress,
  Space,
  message,
  Descriptions,
  Modal,
  Input,
  Empty,
} from 'antd';
import { ThunderboltOutlined, CaretRightOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import { ProcessExecutionBoard } from '@/components/process/ProcessExecutionBoard';
import { complexityColor, complexityLabel, statusColor } from './constants';
import styles from './TaskDetailPanel.module.css';

const { Text, Paragraph } = Typography;

interface TaskDetailPanelProps {
  taskId: number;
  workspaceId: number;
  /** 再次执行成功后回调，让宿主重拉列表保持口径一致。 */
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

/** 门禁类型 → 中文标签，未匹配回退原值。 */
function gateLabel(type: string): string {
  const map: Record<string, string> = {
    artifact_present: '产物存在',
    ai_criteria_review: 'AI 评审',
    human_approval: '人工审批',
    script_check: '脚本校验',
  };
  return map[type] ?? type;
}

/** 执行状态 → Progress 状态语义（success/exception/active/normal）。 */
function progressStatus(status: string): 'success' | 'exception' | 'active' | 'normal' {
  if (status === 'success') return 'success';
  if (status === 'failed') return 'exception';
  if (status === 'running') return 'active';
  return 'normal';
}

/** 顶部条：标题 + 状态/复杂度 + 元信息 + 再次执行主按钮。 */
function DetailHeader({
  task,
  template,
  onExecute,
}: {
  task: TaskDetailData['task'];
  template?: TaskDetailData['template'];
  onExecute: () => void;
}) {
  return (
    <div className={styles.headerBar}>
      {/* 左侧：#id + 标题 + 状态/复杂度 Tag + 模板/版本元信息 */}
      <div className={styles.headerMain}>
        <div className={styles.titleRow}>
          <Text type="secondary">#{task.id}</Text>
          <h2 className={styles.taskTitle}>{task.title}</h2>
          <Tag color={statusColor(task.status)}>{task.status}</Tag>
          {template?.complexity && (
            <Tag color={complexityColor(template.complexity)}>{complexityLabel(template.complexity)}</Tag>
          )}
        </div>
        <div className={styles.metaRow}>
          <span>模板：{template?.display_name ?? '—'}</span>
          <span className={styles.metaDivider}>·</span>
          <span>版本：{template?.version ?? '—'}</span>
        </div>
      </div>
      {/* 右侧：再次执行主操作，跨 Tab 始终可见 */}
      <Button icon={<ThunderboltOutlined />} type="primary" onClick={onExecute}>
        再次执行
      </Button>
    </div>
  );
}

/** 步骤内的一行标签组（技能/产物/门禁）。复用避免每个分支重复结构。 */
function StepMetaRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className={styles.stepMetaRow}>
      <span className={styles.stepLabel}>{label}</span>
      {children}
    </div>
  );
}

/** 单条工艺步骤：圆形序号徽标 + 名称 + 技能/产物/门禁分组。 */
function StepItem({ step }: { step: StepInfo }) {
  return (
    <div className={styles.stepItem}>
      <div className={styles.stepIndex}>{step.order_index + 1}</div>
      <div className={styles.stepBody}>
        <div className={styles.stepName}>{step.name}</div>
        {step.skill_names.length > 0 && (
          <StepMetaRow label="技能">
            {step.skill_names.map((sk) => (
              <Tag key={sk} color="purple" style={{ fontSize: 12 }}>{sk}</Tag>
            ))}
          </StepMetaRow>
        )}
        {step.expected_artifacts.length > 0 && (
          <StepMetaRow label="产物">
            {step.expected_artifacts.map((a, i) => (
              <Tag key={i} color="blue" style={{ fontSize: 12 }}>
                {a.name} → {a.path ?? a.locator} ({a.type})
              </Tag>
            ))}
          </StepMetaRow>
        )}
        {step.gate_config.length > 0 && (
          <StepMetaRow label="门禁">
            {step.gate_config.map((g, i) => (
              <Tag key={i} style={{ fontSize: 12 }}>{g.name} ({gateLabel(g.type)})</Tag>
            ))}
          </StepMetaRow>
        )}
      </div>
    </div>
  );
}

/** 概览 Tab：基本信息 + 需求描述 + 最近一次执行进度。 */
function OverviewTab({
  task,
  template,
  executions,
}: {
  task: TaskDetailData['task'];
  template?: TaskDetailData['template'];
  executions: ExecInfo[];
}) {
  const latest = executions[0];
  const percent = latest && latest.total_steps > 0
    ? Math.round((latest.completed_steps / latest.total_steps) * 100)
    : 0;
  return (
    <div className={styles.paneBody}>
      <Descriptions column={1} size="small" title="基本信息">
        <Descriptions.Item label="模板">{template?.display_name ?? '—'}</Descriptions.Item>
        <Descriptions.Item label="版本">{template?.version ?? '—'}</Descriptions.Item>
        <Descriptions.Item label="复杂度">
          {template?.complexity
            ? <Tag color={complexityColor(template.complexity)}>{complexityLabel(template.complexity)}</Tag>
            : '—'}
        </Descriptions.Item>
        <Descriptions.Item label="状态">
          <Tag color={statusColor(task.status)}>{task.status}</Tag>
        </Descriptions.Item>
      </Descriptions>

      <div style={{ marginTop: 16 }}>
        <Text strong>需求描述</Text>
        <Paragraph style={{ marginTop: 4 }} type={task.description ? undefined : 'secondary'}>
          {task.description || '暂无描述'}
        </Paragraph>
      </div>

      {latest && (
        <div style={{ marginTop: 16 }}>
          <Text strong>最近一次执行 #{latest.id}</Text>
          <Progress percent={percent} status={progressStatus(latest.status)} style={{ marginTop: 8 }} />
          <Text type="secondary">
            完成 {latest.completed_steps}/{latest.total_steps}
            {latest.failed_steps > 0 && ` · 失败 ${latest.failed_steps}`}
          </Text>
        </div>
      )}
    </div>
  );
}

/** 工艺要求 Tab：步骤轻量列表，无「卡片套卡片」。 */
function ProcessTab({ steps }: { steps: StepInfo[] }) {
  if (!steps || steps.length === 0) {
    return <Empty description="暂无工艺步骤" style={{ marginTop: 48 }} />;
  }
  return (
    <div className={styles.paneBody}>
      <div className={styles.steps}>
        {steps.map((s) => <StepItem key={s.id} step={s} />)}
      </div>
    </div>
  );
}

/** 执行历史 Tab：执行列表 + 选中后全宽展开 ProcessExecutionBoard。 */
function ExecTab({
  executions,
  workspaceId,
  loopId,
  activeExec,
  onToggle,
}: {
  executions: ExecInfo[];
  workspaceId: number;
  loopId: number;
  activeExec: number | null;
  onToggle: (id: number) => void;
}) {
  if (!executions || executions.length === 0) {
    return <Empty description="暂无执行" image={Empty.PRESENTED_IMAGE_SIMPLE} style={{ marginTop: 48 }} />;
  }
  return (
    <div className={styles.paneBody}>
      <List
        dataSource={executions}
        renderItem={(e) => (
          <List.Item
            actions={[
              <Button
                key="v"
                type="link"
                icon={<CaretRightOutlined />}
                onClick={() => onToggle(e.id)}
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
              description={e.requirement || <Text type="secondary">{e.started_at ?? ''}</Text>}
            />
          </List.Item>
        )}
      />
      {/* 选中执行的看板：在 Tab 全宽内容区展开，不再被窄栏挤压 */}
      {activeExec != null && (
        <div style={{ marginTop: 16 }}>
          <ProcessExecutionBoard workspaceId={workspaceId} loopId={loopId} executionId={activeExec} />
        </div>
      )}
    </div>
  );
}

/**
 * 任务详情面板（Tabs 版）。
 * 1. 通过 bundledApi.getTaskDetail 一次性拉取任务/模板/步骤/执行历史。
 * 2. 顶部条展示标题/状态/复杂度/模板版本 + 始终可见的「再次执行」。
 * 3. 三个 Tab：概览（信息+最近执行进度）/工艺要求（轻量步骤列表）/执行历史（列表+全宽看板）。
 * 4. 「再次执行」打开 Modal 调 createTaskExecution 创建新执行。
 */
export function TaskDetailPanel({ taskId, workspaceId, onTriggered }: TaskDetailPanelProps) {
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<TaskDetailData | null>(null);
  const [activeExec, setActiveExec] = useState<number | null>(null);
  const [triggering, setTriggering] = useState(false);
  const [reqModalOpen, setReqModalOpen] = useState(false);
  const [newRequirement, setNewRequirement] = useState('');

  // taskId/workspaceId 变化时重拉；卸载时若仍在 loading 不影响已设置状态。
  useEffect(() => {
    let alive = true;
    setLoading(true);
    // 直接拿 any 然后类型断言：bundledApi.getTaskDetail 返回 any，避免引入复杂类型。
    bundledApi
      .getTaskDetail(workspaceId, taskId)
      .then((raw) => {
        if (alive) setDetail(raw as TaskDetailData);
      })
      .catch(() => {
        if (alive) message.error('加载失败');
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId, workspaceId]);

  const openReqModal = () => {
    // 默认把任务描述/标题填入需求框，减少用户重复输入。
    const detail0 = detail;
    if (detail0) setNewRequirement(detail0.task.description ?? detail0.task.title);
    setReqModalOpen(true);
  };

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
      // 重新拉取详情，拿到新执行；并通知宿主刷新列表。
      const raw = (await bundledApi.getTaskDetail(workspaceId, taskId)) as TaskDetailData;
      setDetail(raw);
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
  // workspace/loop id 的回退链：任务自身 → 关联 loop → 默认值。
  const wsId = task.workspace_id ?? detail.loop?.workspace_id ?? 1;
  const lpId = task.loop_id ?? detail.loop?.id ?? 0;

  const tabItems = [
    {
      key: 'overview',
      label: '概览',
      children: <OverviewTab task={task} template={template} executions={executions ?? []} />,
    },
    {
      key: 'process',
      label: `工艺要求 (${steps?.length ?? 0})`,
      children: <ProcessTab steps={steps ?? []} />,
    },
    {
      key: 'exec',
      label: `执行历史 (${executions?.length ?? 0})`,
      children: (
        <ExecTab
          executions={executions ?? []}
          workspaceId={wsId}
          loopId={lpId}
          activeExec={activeExec}
          onToggle={(id) => setActiveExec(activeExec === id ? null : id)}
        />
      ),
    },
  ];

  return (
    <div className={styles.panel}>
      <DetailHeader task={task} template={template} onExecute={openReqModal} />
      <div className={styles.tabsWrap}>
        <Tabs items={tabItems} style={{ height: '100%' }} />
      </div>

      <Modal
        title="输入这次的需求"
        open={reqModalOpen}
        onCancel={() => setReqModalOpen(false)}
        onOk={handleNewExec}
        confirmLoading={triggering}
        okText="开始执行"
      >
        <Input.TextArea
          value={newRequirement}
          onChange={(e) => setNewRequirement(e.target.value)}
          rows={4}
        />
      </Modal>
    </div>
  );
}
