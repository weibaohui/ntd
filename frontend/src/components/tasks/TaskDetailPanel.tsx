// 嵌入式任务详情面板（合并环路详情版）。
// 任务与环路是 1:1 关系（task.loop_id → loop.id），本面板合并两个详情页：
// - Tab 1 概览：任务描述 + 环路基本信息（工作空间/待审批）+ 全局限制 + 最新执行进度
// - Tab 2 执行环路：来源工艺面包屑 + SVG DAG 流程图（复用 LoopFlowGraph）
// - Tab 3 执行历史：分页执行列表 + TokenSummaryBar + StepExecList + BlackboardDrawer
//
// 子组件（Tab 内容）拆分至 TaskDetailTabs.tsx 以控制文件大小。

import { useEffect, useState, useCallback } from 'react';
import {
  Tabs, Tag, Button, Typography, Spin, Space, Badge,
  message, Modal, Input, Empty, Popconfirm,
} from 'antd';
import {
  ThunderboltOutlined, DeleteOutlined,
} from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import * as dbLoops from '@/utils/database/loops';
import { useProjectDirectories } from '@/utils/workspaceDisplay';
import type { LoopDetail } from '@/types/loop';
import { complexityColor, complexityLabel, statusColor } from './constants';
import {
  OverviewTab, DAGTab, ExecHistoryTab,
} from './TaskDetailTabs';
import type { StepInfo } from './TaskDetailTabs';
import { DiscussionTab } from './discussion/DiscussionTab';
import styles from './TaskDetailPanel.module.css';

const { Text } = Typography;

// ====== 类型定义 ======

interface TaskDetailPanelProps {
  taskId: number;
  workspaceId: number;
  /** 再次执行成功后回调，让宿主重拉列表。 */
  onTriggered?: () => void;
  /** 任务标题加载完成后回调，供外层 PageCard 动态更新标题。 */
  onTitleReady?: (title: string) => void;
  /** 点击 DAG 节点上的事项标题跳转事项详情。 */
  onOpenTodo?: (todoId: number) => void;
  /** 环路状态变更（启停/删除）后通知宿主刷新列表。 */
  onLoopChanged?: () => void;
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
  pending_approval_count?: number;
}

interface TaskDetailData {
  task: { id: number; title: string; status: string; description?: string; workspace_id?: number; loop_id?: number };
  template?: { display_name?: string; version?: string; complexity?: string };
  steps: StepInfo[];
  executions: ExecInfo[];
  loop?: { id: number; workspace_id?: number };
}

// ====== 子组件 ======

/** 顶部条：标题 + 状态/复杂度 + 元信息 + 删除 + 再次执行。 */
function DetailHeader({
  task, template, loopDetail, onExecute, onDelete,
}: {
  task: TaskDetailData['task'];
  template?: TaskDetailData['template'];
  loopDetail: LoopDetail | null;
  onExecute: () => void;
  onDelete: () => void;
}) {
  return (
    <div className={styles.headerBar}>
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
      <Space>
        {loopDetail && (
          <Popconfirm title="确定删除此环路？" onConfirm={onDelete} okText="删除" cancelText="取消">
            <Button icon={<DeleteOutlined />} danger size="small">删除</Button>
          </Popconfirm>
        )}
        <Button icon={<ThunderboltOutlined />} type="primary" onClick={onExecute}>再次执行</Button>
      </Space>
    </div>
  );
}

// ====== 主组件 ======

/**
 * 任务详情面板（合并环路详情版）。
 * 数据获取：先拉任务详情 → 有 loop_id 则并行拉完整 LoopDetail。
 * Tab 结构：概览 / 执行环路(DAG) / 执行历史。
 */
export function TaskDetailPanel({
  taskId, workspaceId, onTriggered, onTitleReady,
  onOpenTodo, onLoopChanged,
}: TaskDetailPanelProps) {
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<TaskDetailData | null>(null);
  const [loopDetail, setLoopDetail] = useState<LoopDetail | null>(null);
  const [loopLoading, setLoopLoading] = useState(false);
  const [triggering, setTriggering] = useState(false);
  const [reqModalOpen, setReqModalOpen] = useState(false);
  const [newRequirement, setNewRequirement] = useState('');
  const { dirs: projectDirs } = useProjectDirectories();

  // 拉取任务详情（含基本 loop 信息）。
  useEffect(() => {
    let alive = true;
    setLoading(true);
    bundledApi.getTaskDetail(workspaceId, taskId)
      .then((raw) => {
        if (!alive) return;
        const d = raw as TaskDetailData;
        setDetail(d);
        if (onTitleReady && d.task?.title) onTitleReady(d.task.title);
      })
      .catch(() => { if (alive) message.error('加载任务详情失败'); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId, workspaceId]);

  // 任务详情加载后，若有 loop_id 则并行拉取完整 LoopDetail。
  useEffect(() => {
    if (!detail) return;
    const lpId = detail.task.loop_id ?? detail.loop?.id;
    if (!lpId) return;
    let alive = true;
    setLoopLoading(true);
    const wsId = detail.task.workspace_id ?? detail.loop?.workspace_id ?? workspaceId;
    dbLoops.getLoop(wsId, lpId)
      .then((ld) => { if (alive) setLoopDetail(ld); })
      .catch(() => { /* 环路加载失败不影响任务展示 */ })
      .finally(() => { if (alive) setLoopLoading(false); });
    return () => { alive = false; };
  }, [detail, workspaceId]);

  // 删除环路。
  const handleDelete = useCallback(async () => {
    if (!loopDetail) return;
    const wsId = loopDetail.workspace_id ?? workspaceId;
    try {
      await dbLoops.deleteLoop(wsId, loopDetail.id);
      message.success('已删除');
      onLoopChanged?.();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  }, [loopDetail, workspaceId, onLoopChanged]);

  // 打开再次执行 Modal。
  const openReqModal = () => {
    if (detail) setNewRequirement(detail.task.description ?? detail.task.title);
    setReqModalOpen(true);
  };

  // 提交新执行。
  const handleNewExec = async () => {
    if (!newRequirement.trim()) { message.warning('请输入需求'); return; }
    setTriggering(true);
    try {
      await bundledApi.createTaskExecution(workspaceId, taskId, newRequirement);
      message.success('新执行已创建');
      setReqModalOpen(false);
      setNewRequirement('');
      const raw = await bundledApi.getTaskDetail(workspaceId, taskId) as TaskDetailData;
      setDetail(raw);
      onTriggered?.();
    } catch { message.error('创建失败'); }
    finally { setTriggering(false); }
  };

  // 加载态。
  if (loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;
  if (!detail) return <Empty description="暂无任务详情" style={{ marginTop: 48 }} />;

  const { task, template } = detail;
  const lpId = task.loop_id ?? detail.loop?.id ?? 0;
  const lpWsId = task.workspace_id ?? detail.loop?.workspace_id ?? null;

  // 讨论区 running 帖数量（DiscussionTab 上报），用于「讨论」Tab 角标（M4）。
  const [discussionRunning, setDiscussionRunning] = useState(0);

  const tabItems = [
    {
      key: 'overview',
      label: '概览',
      children: (
        <OverviewTab
          task={task} template={template}
          loopDetail={loopDetail} projectDirs={projectDirs}
        />
      ),
    },
    {
      key: 'dag',
      label: `执行环路 (${loopDetail?.steps?.length ?? 0})`,
      children: <DAGTab loopDetail={loopDetail} steps={detail.steps ?? []} onOpenTodo={onOpenTodo} />,
    },
    {
      key: 'exec',
      label: '执行历史',
      children: (
        <ExecHistoryTab loopId={lpId} workspaceId={lpWsId} loopName={loopDetail?.name ?? task.title} />
      ),
    },
    {
      // 任务讨论区（需求 060）：论坛跟帖 + @专家/@执行器 触发执行后回帖。
      // forceRender：保证「讨论」Tab 非 active 时 DiscussionTab 仍挂载、持续上报 running 数，角标才可见。
      key: 'discussion',
      label: <Badge count={discussionRunning} offset={[10, 0]} size="small">讨论</Badge>,
      forceRender: true,
      children: (
        <DiscussionTab
          taskId={task.id}
          workspaceId={task.workspace_id ?? lpWsId ?? workspaceId}
          onRunningCountChange={setDiscussionRunning}
        />
      ),
    },
  ];

  return (
    <div className={styles.panel}>
      <DetailHeader
        task={task} template={template} loopDetail={loopDetail}
        onExecute={openReqModal} onDelete={handleDelete}
      />
      <div className={styles.tabsWrap}>
        <Tabs
          items={tabItems}
          style={{ height: '100%' }}
          tabBarExtraContent={loopLoading ? <Spin size="small" style={{ marginRight: 16 }} /> : undefined}
        />
      </div>

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
