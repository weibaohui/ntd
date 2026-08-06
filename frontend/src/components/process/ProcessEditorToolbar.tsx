// ProcessEditorToolbar.tsx
// ---------------------------------------------------------------------------
// M5 里程碑：工艺编辑器顶部工具栏。
//
// 设计意图（对应 docs/design/029-M5-双向联动与保存-方案.md §3.1.1 + 设计 §8）：
// - 左侧：工艺名 + display_name
// - 右侧：「未保存修改」红点（isDirty 时显示）+ 保存按钮 + 删除按钮（仅 !isSystem）
//
// 交互：
// - 保存按钮：disabled=isSaving||isSystem，点击调 onSave
// - 删除按钮：仅 !isSystem 渲染，disabled=isDeleting，点击调 onDelete（onDelete 内弹 Modal.confirm）
//
// 数据流：纯展示组件，所有状态由父 ProcessEditor 注入，回调向上转发。
// ---------------------------------------------------------------------------

import { type CSSProperties, type JSX } from 'react';
import { Button, Space, Typography } from 'antd';
import {
  SaveOutlined,
  DeleteOutlined,
  ExclamationCircleOutlined,
  ArrowLeftOutlined,
} from '@ant-design/icons';

const { Text } = Typography;

export interface ProcessEditorToolbarProps {
  // 工艺名（YAML name 字段，路由参数）
  processName: string;
  // 工艺显示名（display_name，可空）
  displayName?: string;
  // 是否系统工艺（系统工艺禁用保存 + 不渲染删除按钮）
  isSystem: boolean;
  // 是否有未保存修改（控制红点显示）
  isDirty: boolean;
  // 保存中状态（控制按钮禁用 + loading）
  isSaving: boolean;
  // 删除中状态（控制按钮禁用 + loading）
  isDeleting: boolean;
  // 保存回调（父组件调 bundledApi.putProcess）
  onSave: () => void;
  // 删除回调（父组件弹 Modal.confirm 后调 bundledApi.deleteProcess）
  onDelete: () => void;
  // 返回回调（父组件跳路由回工艺列表页；沿用了离开拦截的 hashchange 监听）
  onBack: () => void;
}

// Toolbar 组件实现。
//
// 纯展示：不持有状态，所有 props 由父注入。
// 布局：横向 flex，左标题右操作区，底部边框分隔。
export function ProcessEditorToolbar({
  processName,
  displayName,
  isSystem,
  isDirty,
  isSaving,
  isDeleting,
  onSave,
  onDelete,
  onBack,
}: ProcessEditorToolbarProps): JSX.Element {
  return (
    <div style={toolbarStyle}>
      {/* 左：062 起标题统一「模块名: 具体名称」格式；displayName 与唯一名同时保留，
          唯一名作次级文本便于区分同名显示名的工艺。 */}
      <div style={titleStyle}>
        <Text strong>工艺: {displayName ?? processName}</Text>
        {displayName && (
          <Text type="secondary" style={nameTextStyle}>
            ({processName})
          </Text>
        )}
      </div>

      {/* 右：未保存红点 + 保存 + 删除 */}
      <Space size="middle">
        {/* 未保存修改红点提示，仅 isDirty 时显示 */}
        {isDirty && (
          <Text type="danger" style={dirtyStyle}>
            <ExclamationCircleOutlined /> 未保存修改
          </Text>
        )}

        {/* 保存按钮：系统工艺禁用（兜底，系统工艺 Alert 已提示） */}
        <Button
          type="primary"
          icon={<SaveOutlined />}
          onClick={onSave}
          disabled={isSaving || isSystem}
          loading={isSaving}
        >
          保存
        </Button>

        {/* 删除按钮：仅用户工艺渲染（系统工艺后端也会拒绝） */}
        {!isSystem && (
          <Button
            danger
            icon={<DeleteOutlined />}
            onClick={onDelete}
            disabled={isDeleting}
            loading={isDeleting}
          >
            删除
          </Button>
        )}

        {/* 返回按钮：右上角（操作区最右端，与全站 PageCard onBack 位置一致，062）。
            样式与 PageCard 统一返回按钮对齐：small + text + 左箭头。
            点击跳回工艺列表页（#/processes），经由父组件设置 location.hash，
            复用 hashchange 离开拦截，有未保存修改时会自动弹确认框，避免误丢改动。 */}
        <Button size="small" type="text" icon={<ArrowLeftOutlined />} onClick={onBack}>
          返回列表
        </Button>
      </Space>
    </div>
  );
}

// ── 样式 ──────────────────────────────────────────

// 工具栏底色与边框用主题变量：亮色白底 slate 边，暗色随 data-theme 切到 Mocha 底色，
// 避免暗色主题下工具栏仍是一块写死的白底
const toolbarStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  padding: '8px 16px',
  borderBottom: '1px solid var(--color-border)',
  background: 'var(--color-bg-elevated)',
};

const titleStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'baseline',
  gap: 8,
};

const nameTextStyle: CSSProperties = {
  fontSize: 12,
};

const dirtyStyle: CSSProperties = {
  fontSize: 13,
  display: 'inline-flex',
  alignItems: 'center',
  gap: 4,
};
