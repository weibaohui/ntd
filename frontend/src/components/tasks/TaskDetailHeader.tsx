// 任务详情顶部条（从 TaskDetailPanel.tsx 拆分，需求 093 重构）。
//
// 含：纯函数 isAutoRelayTask / assigneeLabel + 组件 RelayBadge / RelayMaxEditor / DetailHeader。
// 逐行平移自 TaskDetailPanel.tsx，逻辑无变更；styles 复用面板的 CSS Module。
//
// 拆分动机：这些头部展示组件与「面板」的数据编排无关，原与主组件同处一文件使
// TaskDetailPanel 长达 470 行。外迁后面板回归薄编排层，头部可独立阅读/复用。

import { useState } from 'react';
import {
  Tag, Button, Typography, Space, InputNumber, Popover, Popconfirm,
} from 'antd';
import {
  ThunderboltOutlined, DeleteOutlined, EditOutlined,
} from '@ant-design/icons';
import type { TaskDetailData } from '@/types/task';
import { complexityColor, complexityLabel, statusColor } from './constants';
import styles from './TaskDetailPanel.module.css';

const { Text } = Typography;

// ====== Props 类型（03-组件编写规范 §2：具名 <Component>Props interface）======

/** 接力徽标入参：任务数据（读取接力态）+ 上限落库回调（透传给内联编辑器）。 */
interface RelayBadgeProps {
  task: TaskDetailData['task'];
  onUpdateMax: (max: number | null) => Promise<void>;
}

/** 徽标内联编辑器入参：与 RelayBadge 一致（task + 落库回调）。 */
interface RelayMaxEditorProps {
  task: TaskDetailData['task'];
  onUpdateMax: (max: number | null) => Promise<void>;
}

/** 顶部条入参：任务 / 工艺元数据 + 删除 / 再次执行 / 调上限回调。 */
interface DetailHeaderProps {
  task: TaskDetailData['task'];
  template?: TaskDetailData['template'];
  onExecute: () => void;
  onDelete: () => void;
  // 接力上限内联编辑落库（透传给 RelayBadge）；仅管家接力任务的徽标会用。
  onUpdateMax: (max: number | null) => Promise<void>;
}

// ====== 纯函数 ======

/** 是否为「管家自动接力」任务：委派 + 开启自动接力 + 专家处理人（执行器 P1 已禁用接力）。 */
function isAutoRelayTask(task: TaskDetailData['task']): boolean {
  return task.execution_mode === 'delegate'
    && !!task.auto_continue
    && task.assignee_kind === 'expert';
}

/** 委派任务处理人展示文案：「专家/执行器 名称」；缺名回退「—」。 */
function assigneeLabel(task: TaskDetailData['task']): string {
  const kind = task.assignee_kind === 'expert' ? '专家'
    : task.assignee_kind === 'executor' ? '执行器' : '处理人';
  return task.assignee_name ? `${kind} ${task.assignee_name}` : '—';
}

// ====== 子组件 ======

/**
 * 自动接力进度徽标（需求 092）。仅管家接力任务显示，文案「管家调度中 N/M」：
 * - M（上限）来自后端三级解析的有效值 delegate_max_rounds_effective，前端不再硬编码 10；
 * - 未达上限用 processing 蓝，达上限用 warning 橙（与后端 HitLimit 说明帖呼应，提示接管）；
 * - 点击 ✎ 弹出 [RelayMaxEditor] 内联调整该任务的上限覆盖（运行时可改）。
 * 非接力任务返回 null。本组件不放 hooks：纯条件渲染，避免「early return 在 hook 之前」违规。
 */
function RelayBadge({
  task, onUpdateMax,
}: RelayBadgeProps) {
  if (!isAutoRelayTask(task)) return null;
  return <RelayMaxEditor task={task} onUpdateMax={onUpdateMax} />;
}

/**
 * 徽标内联编辑器：Tag 本体 + Popover（InputNumber 调上限 + 「恢复默认」）。
 * 打开时以当前 raw 覆盖(delegate_max_rounds)回显，null 显示空（=用默认）；确定/恢复均经
 * onUpdateMax 落库并重拉详情，effective 随之刷新，徽标 M 实时同步。
 *
 * 【函数长度豁免】函数体超 50 行，主体为声明式 Popover/Tag 的 JSX 渲染（CLAUDE.md
 * 「纯数据构建」类豁免）；少量受控态（open/editing/saving）与 submit 事件处理与该 JSX
 * 紧耦合，抽成独立子组件须在父子间传递 6+ 个受控 setter，反而增加传参与跳转阅读成本。
 */
function RelayMaxEditor({
  task, onUpdateMax,
}: RelayMaxEditorProps) {
  const rounds = task.continue_rounds ?? 0;
  // effective 兜底 10 仅为类型安全：后端恒返回该字段，缺失属异常态。
  const effectiveMax = task.delegate_max_rounds_effective ?? 10;
  // 「清除任务覆盖后」的回退值（工作空间默认或兜底常量）：placeholder / 留空提示读它。
  // 不能用 effective：任务已有覆盖时 effective 即覆盖值本身，提示「留空=用工作空间默认（N 轮）」会误显成
  // 即将被清除的那个覆盖值，与「清除后实际回退到工作空间默认」矛盾（Spec 评审发现）。
  const fallbackMax = task.delegate_max_rounds_fallback ?? effectiveMax;
  // >=：与后端 plan_delegate_relay 的 `rounds >= max` 护栏口径一致（设计 §5.2）。
  const atLimit = rounds >= effectiveMax;
  // Popover 受控开关 + 输入态：每次打开以最新 raw 回显，避免上一次残留值误导。
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<number | null>(task.delegate_max_rounds ?? null);
  const [saving, setSaving] = useState(false);

  // 落库并刷新：仅当 onUpdateMax 成功 resolve 才关 Popover；失败时 onUpdateMax 会向上抛错，
  // 跳过 setOpen(false)，编辑器保持打开供改值重试（错误提示已由 handleUpdateMax 的 catch 弹出）。
  // 此处 catch 兜住错误只复位 saving，避免 onClick 内产生未处理 promise rejection。
  const submit = async (value: number | null) => {
    setSaving(true);
    try {
      await onUpdateMax(value);
      setOpen(false);
    } catch {
      // 不关 Popover：让用户在原值基础上修正后重试（message.error 已在 handleUpdateMax 弹过）。
    } finally {
      setSaving(false);
    }
  };
  // 是否已有覆盖（决定「恢复默认」可点）；确定仅在「填了且与当前覆盖不同」时可点。
  const hasOverride = task.delegate_max_rounds != null;
  const confirmDisabled = editing == null || editing === task.delegate_max_rounds;

  return (
    <Popover
      trigger="click"
      placement="bottom"
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        // 每次打开重置输入为当前 raw 覆盖，关闭再开不会带上次未提交的脏值。
        if (o) setEditing(task.delegate_max_rounds ?? null);
      }}
      title="调整接力轮数上限"
      content={
        <Space direction="vertical" style={{ width: 240 }}>
          <InputNumber
            min={1}
            max={50}
            value={editing}
            onChange={(v) => setEditing(v ?? null)}
            placeholder={`默认 ${fallbackMax} 轮`}
            style={{ width: '100%' }}
            data-testid="relay-max-input"
          />
          <Space>
            <Button
              size="small"
              type="primary"
              loading={saving}
              disabled={confirmDisabled}
              onClick={() => editing != null && submit(editing)}
              data-testid="relay-max-confirm"
            >
              确定
            </Button>
            <Button
              size="small"
              disabled={!hasOverride || saving}
              onClick={() => submit(null)}
              data-testid="relay-max-reset"
            >
              恢复默认
            </Button>
          </Space>
          <Text type="secondary" style={{ fontSize: 12 }}>
            留空=用工作空间默认（{fallbackMax} 轮）；达上限后停止自动接力。
          </Text>
        </Space>
      }
    >
      <Tag
        color={atLimit ? 'warning' : 'processing'}
        style={{ cursor: 'pointer' }}
        data-testid="relay-badge"
      >
        管家调度中 {rounds}/{effectiveMax} <EditOutlined style={{ marginLeft: 4 }} />
      </Tag>
    </Popover>
  );
}

/**
 * 顶部条：标题 + 状态/复杂度 + 元信息 + 删除 + 再次执行。
 *
 * 【函数长度豁免】函数体超 50 行纯属声明式 JSX（CLAUDE.md「纯数据构建」类豁免）——
 * 逐行平移自原 TaskDetailPanel，无分支逻辑，拆分只会割裂「顶部条」这一整体视觉单元。
 */
export function DetailHeader({
  task, template, onExecute, onDelete, onUpdateMax,
}: DetailHeaderProps) {
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
          {/* 管家自动接力任务的进度徽标（委派 + 自动接力 + 专家）。非此类任务返回 null。 */}
          <RelayBadge task={task} onUpdateMax={onUpdateMax} />
        </div>
        <div className={styles.metaRow}>
          {task.execution_mode === 'delegate' ? (
            // 委派任务无工艺/版本概念，改展示处理人（专家/执行器 + 名称）。
            <>
              <span>处理人：{assigneeLabel(task)}</span>
            </>
          ) : (
            <>
              <span>工艺：{template?.display_name ?? '—'}</span>
              <span className={styles.metaDivider}>·</span>
              <span>版本：{template?.version ?? '—'}</span>
            </>
          )}
        </div>
      </div>
      <Space>
        {/* NTD-014-A：删除按钮删除任务本身（原实现误删关联环路）。
            所有任务（含委派）都可删除，不再依赖 loopDetail 显隐。 */}
        <Popconfirm title="确定删除任务？" onConfirm={onDelete} okText="删除" cancelText="取消">
          <Button icon={<DeleteOutlined />} danger size="small">删除</Button>
        </Popconfirm>
        {/* 委派任务无环路执行概念，「再次执行」走 loop 路径不适用；用户在讨论区 @ 继续推进。 */}
        {task.execution_mode !== 'delegate' ? (
          <Button icon={<ThunderboltOutlined />} type="primary" onClick={onExecute}>再次执行</Button>
        ) : null}
      </Space>
    </div>
  );
}
