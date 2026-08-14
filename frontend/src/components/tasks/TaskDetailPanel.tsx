// 嵌入式任务详情面板（薄编排层）。
//
// 本面板只做三件事：拉数据（委托 useTaskDetail）、组装 Tab、渲染顶部条 + Tabs + 再次执行 Modal。
// 数据/副作用逻辑见 @/hooks/useTaskDetail；顶部条（标题/状态/接力徽标/删除/再次执行）见
// ./TaskDetailHeader；Tab 内容子组件（概览/执行环路/执行历史）见 ./TaskDetailTabs。
// Tab 显隐纯逻辑见 ./helpers（visibleTaskTabs / resolveTaskActiveTab）。

import { useState } from 'react';
import { Tabs, Spin, Empty, Badge, Modal, Input } from 'antd';
import { useTaskDetail } from '@/hooks/useTaskDetail';
import { useWorkspaces } from '@/utils/workspaceDisplay';
import { useViewState } from '@/hooks/useViewState';
import { visibleTaskTabs, resolveTaskActiveTab } from './helpers';
import { DetailHeader } from './TaskDetailHeader';
import { OverviewTab, DAGTab, ExecHistoryTab } from './TaskDetailTabs';
import { DiscussionTab } from './discussion/DiscussionTab';
import styles from './TaskDetailPanel.module.css';

interface TaskDetailPanelProps {
  taskId: number;
  workspaceId: number;
  /** 再次执行成功后回调，让宿主重拉列表。 */
  onTriggered?: () => void;
  /** 任务标题加载完成后回调，供外层 PageCard 动态更新标题。 */
  onTitleReady?: (title: string) => void;
  /** 点击 DAG 节点上的事项标题跳转事项详情。 */
  onOpenTodo?: (todoId: number) => void;
  /** 任务删除成功后回调，由宿主跳回任务列表（NTD-014-A）。 */
  onDeleted?: () => void;
}

/**
 * 任务详情面板（薄编排层）。
 * 数据获取与动作全权委托 useTaskDetail；本组件只负责 Tab 组装与渲染。
 */
export function TaskDetailPanel({
  taskId, workspaceId, onTriggered, onTitleReady,
  onOpenTodo, onDeleted,
}: TaskDetailPanelProps) {
  // 数据层：拉详情/拉环路、删除/再次执行/调接力上限，全部封装在 hook 内（可单测）。
  const t = useTaskDetail(taskId, workspaceId, { onTitleReady, onTriggered, onDeleted });
  // 讨论区 running 帖数量（DiscussionTab 上报），用于「讨论」Tab 角标。
  // 纯展示态，不进 hook（与任务数据无关）；声明在 early return 之前以满足 hooks 顺序规则。
  const [discussionRunning, setDiscussionRunning] = useState(0);
  // 再次执行 Modal 的 UI 态（开关 / 输入文案）：纯 UI，由组件持有；提交动作委托 hook。
  // 同样声明在 early return 之前，保证 loading 提前 return 时 hooks 顺序稳定。
  const [reqModalOpen, setReqModalOpen] = useState(false);
  const [newRequirement, setNewRequirement] = useState('');
  const { dirs: workspaces } = useWorkspaces();
  // URL ?tab= 驱动 Tabs 选中态（对齐 Settings 页模式）：帖子页返回任务-讨论 tab 时
  // 返回 URL 带 ?tab=discussion，此处解析出 activeTab 落到对应 Tab；非法值回退「概览」。
  // 切 tab 用 replaceUrl：只更新 URL 不压 history，避免浏览器后退逐个回退 tab 而非离开页面。
  const { activeTab, replaceUrl } = useViewState();

  // 加载态。
  if (t.loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;
  if (!t.detail) return <Empty description="暂无任务详情" style={{ marginTop: 48 }} />;

  const { task, template } = t.detail;
  const lpId = task.loop_id ?? t.detail.loop?.id ?? 0;
  const lpWsId = task.workspace_id ?? t.detail.loop?.workspace_id ?? null;

  // 声明式列出全部 4 个 Tab 的内容定义；具体展示哪些由 visibleTaskTabs 按执行方式过滤。
  // 「内容定义」与「显隐规则」解耦：显隐属纯逻辑，抽到 helpers.ts 可单测，组件只管渲染。
  const allTabs = [
    {
      key: 'overview',
      label: '概览',
      children: (
        <OverviewTab
          task={task} template={template}
          loopDetail={t.loopDetail} workspaces={workspaces}
        />
      ),
    },
    {
      // 「执行环路」强依赖 task.loop_id：仅工艺环路任务（execution_mode==='loop'）有数据；
      // 委派任务该项会在下方过滤阶段被剔除，内容定义无需关心显隐。
      key: 'dag',
      label: `执行环路 (${t.loopDetail?.steps?.length ?? 0})`,
      children: <DAGTab loopDetail={t.loopDetail} steps={t.detail.steps ?? []} onOpenTodo={onOpenTodo} />,
    },
    {
      // 「执行历史」同样依赖 loop_id，与「执行环路」同显同隐。
      key: 'exec',
      label: '执行历史',
      children: (
        <ExecHistoryTab loopId={lpId} workspaceId={lpWsId} loopName={t.loopDetail?.name ?? task.title} />
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

  // 按执行方式过滤出可见 Tab：委派任务剔除 dag/exec（无 loop_id，渲染只剩空状态）。
  // 顺序仍遵循 allTabs 的声明顺序（visibleTaskTabs 仅作成员集合判断）。
  const visibleKeys = visibleTaskTabs(task.execution_mode);
  const tabItems = allTabs.filter((tab) => (visibleKeys as readonly string[]).includes(tab.key));

  // 解析当前生效 Tab：URL ?tab= 偏好若非法或被隐藏（如委派任务残留 ?tab=dag），回退默认 Tab。
  // 白名单校验与执行方式回退统一由 resolveTaskActiveTab 承担（按可见集合校验 + 回退默认），
  // 组件不再重复首轮兜底；activeTab 为 null（无 ?tab）时传 undefined 即走回退分支。
  const activeTabKey = resolveTaskActiveTab(activeTab ?? undefined, task.execution_mode);

  // 打开再次执行 Modal：以任务描述（或缺省标题）预填输入框。
  const openReqModal = () => {
    setNewRequirement(t.detail?.task.description ?? t.detail?.task.title ?? '');
    setReqModalOpen(true);
  };
  // 提交新执行：成功才关 Modal + 清输入；空需求 / 失败保持打开供修正（与原行为一致）。
  const handleOk = async () => {
    if (await t.handleNewExec(newRequirement)) {
      setReqModalOpen(false);
      setNewRequirement('');
    }
  };

  return (
    <div className={styles.panel}>
      <DetailHeader
        task={task} template={template}
        onExecute={openReqModal} onDelete={t.handleDelete} onUpdateMax={t.handleUpdateMax}
      />
      <div className={styles.tabsWrap}>
        <Tabs
          items={tabItems}
          activeKey={activeTabKey}
          onChange={(key) => replaceUrl('tasks', { id: task.id, tab: key })}
          style={{ height: '100%' }}
          tabBarExtraContent={t.loopLoading ? <Spin size="small" style={{ marginRight: 16 }} /> : undefined}
        />
      </div>

      <Modal
        title="输入这次的需求"
        open={reqModalOpen}
        onCancel={() => setReqModalOpen(false)}
        onOk={handleOk}
        confirmLoading={t.triggering}
        okText="开始执行"
      >
        <Input.TextArea value={newRequirement} onChange={(e) => setNewRequirement(e.target.value)} rows={4} />
      </Modal>
    </div>
  );
}
