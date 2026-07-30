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
import type { GateDefinition } from '@/types/process';
import { complexityColor, complexityLabel, statusColor } from './constants';
import styles from './TaskDetailPanel.module.css';

const { Text, Paragraph } = Typography;

interface TaskDetailPanelProps {
  taskId: number;
  workspaceId: number;
  /** 再次执行成功后回调，让宿主重拉列表保持口径一致。 */
  onTriggered?: () => void;
  /** 任务标题加载完成后回调，供外层 PageCard 动态更新标题（详情标题功能）。 */
  onTitleReady?: (title: string) => void;
}

interface StepInfo {
  id: number;
  name: string;
  order_index: number;
  skill_names: string[];
  expected_artifacts: Array<{ name: string; path?: string; locator?: string; type: string }>;
  // 后端 get_task_detail 返回的是完整 gate_config JSON；这里复用工艺类型，
  // 让 AI 评审门禁的 min_score 等判定条件能在工艺要求 tab 展示出来。
  gate_config: GateDefinition[];
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
  /** 待人工审批的环节数：>0 时在执行历史行上显示引导标记（NTD-004）。 */
  pending_approval_count?: number;
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

/** 把单条门禁的关键判定条件拼成短文本；无额外参数时返回空串，保持 Tag 简洁。 */
function gateDetailText(gate: GateDefinition): string {
  const parts: string[] = [];
  // AI 评审门禁必须暴露通过阈值，否则用户无法理解「评分达标」到底要求多少分。
  if (gate.type === 'ai_criteria_review' && typeof gate.min_score === 'number') {
    parts.push(`阈值 ≥ ${gate.min_score} 分`);
  }
  // timeout_secs 仅正数有意义：表示最多等待 AI 出分的秒数，0/null 不展示。
  if (gate.type === 'ai_criteria_review' && typeof gate.timeout_secs === 'number' && gate.timeout_secs > 0) {
    parts.push(`等待 ≤ ${gate.timeout_secs}s`);
  }
  // 产物存在门禁的补充信息是产物名；script_check 的补充信息是脚本路径。
  if (gate.type === 'artifact_present' && gate.artifact) parts.push(`产物 ${gate.artifact}`);
  if (gate.type === 'script_check' && gate.script) parts.push(`脚本 ${gate.script}`);
  return parts.join('；');
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
          <span>工艺：{template?.display_name ?? '—'}</span>
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
            {step.gate_config.map((g, i) => {
              // 有判定参数时追加到类型标签后面，例如 AI 评审会显示「阈值 ≥ 80 分」。
              const detail = gateDetailText(g);
              const suffix = detail ? `${gateLabel(g.type)} · ${detail}` : gateLabel(g.type);
              return <Tag key={i} style={{ fontSize: 12 }}>{g.name} ({suffix})</Tag>;
            })}
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
        <Descriptions.Item label="工艺模板">{template?.display_name ?? '—'}</Descriptions.Item>
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

/** 执行历史 Tab：每项可内联展开，看板在表头正下方、同框成整体。 */
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
    // execList：纵向排列的折叠项；每项是一个带边框的卡片（整体框）。
    <div className={`${styles.paneBody} ${styles.execList}`}>
      {executions.map((e) => {
        // 该执行是否处于展开态：决定箭头旋转与详情框显隐。
        const expanded = activeExec === e.id;
        return (
          <div
            key={e.id}
            className={expanded ? `${styles.execItem} ${styles.execItemExpanded}` : styles.execItem}
          >
            {/* 整行可点：表头 + 右侧展开指示。role=button 保证键盘可达。 */}
            <div
              className={styles.execRow}
              role="button"
              tabIndex={0}
              aria-expanded={expanded}
              aria-label={`执行 #${e.id} ${expanded ? '收起详情' : '查看详情'}`}
              onClick={() => onToggle(e.id)}
              onKeyDown={(ev) => {
                // 回车/空格触发展开，与鼠标点击等价（可访问性）。
                if (ev.key === 'Enter' || ev.key === ' ') {
                  ev.preventDefault();
                  onToggle(e.id);
                }
              }}
            >
              <div className={styles.execRowMain}>
                <Space>
                  <Tag color={statusColor(e.status)}>{e.status}</Tag>
                  <Text>#{e.id} {e.completed_steps}/{e.total_steps} 完成</Text>
                  {e.failed_steps > 0 && <Tag color="orange">失败 {e.failed_steps}</Tag>}
                  {/* 待审批引导：审批按钮在展开后的工艺看板里，
                      此处用醒目 Tag 告诉用户"需要展开处理"，否则 loop 会永久停在该环节。 */}
                  {(e.pending_approval_count ?? 0) > 0 && (
                    <Tag color="warning">⏳ {e.pending_approval_count} 条待审批，展开处理</Tag>
                  )}
                </Space>
                <div className={styles.execRowDesc}>
                  {e.requirement || <Text type="secondary">{e.started_at ?? ''}</Text>}
                </div>
              </div>
              {/* 右侧：旋转的箭头 + 文案，明确当前是展开还是收起。 */}
              <div className={styles.execRowAction}>
                <span className={styles.execChevron} data-expanded={expanded}>
                  <CaretRightOutlined />
                </span>
                <Text type="secondary">{expanded ? '收起' : '查看详情'}</Text>
              </div>
            </div>
            {/* 内联展开区：与表头同框（外层卡片 border + overflow:hidden），
                顶部一条分隔线 + 略深底色，视觉上属于同一整体单元。 */}
            {expanded && (
              <div className={styles.execDetail}>
                <ProcessExecutionBoard workspaceId={workspaceId} loopId={loopId} executionId={e.id} />
              </div>
            )}
          </div>
        );
      })}
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
export function TaskDetailPanel({ taskId, workspaceId, onTriggered, onTitleReady }: TaskDetailPanelProps) {
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
        // 详情标题：数据加载后通知外层 PageCard 更新标题。
        if (alive && onTitleReady && (raw as TaskDetailData).task?.title) {
          onTitleReady((raw as TaskDetailData).task.title);
        }
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
