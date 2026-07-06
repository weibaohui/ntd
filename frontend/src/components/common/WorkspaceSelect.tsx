// 工作空间选择器：统一「选择已有目录」和「快捷新建」于一身。
//
// 复用场景：
// - TodoDrawer：替换原有的 AutoComplete + Button 组合
// - LoopFormModal：替换原有的 Select（添加快捷新建能力）
//
// 交互逻辑：
// 1. Select 下拉展示目录列表，支持搜索过滤
// 2. 旁边的「+」按钮打开新建 Modal
// 3. 填写名称 + 路径后「保存并使用」，自动选中新建项并通知父组件
// 4. 新建失败时保留表单内容，允许用户修正后重试
//
// 约定（破坏式更新）：
// - value / onChange 全部以 project_directories.id（number）为唯一键传递；
//   path 仅用于下拉项的展示文本（name 优先、path 兜底）。
// - 新增目录成功后通过 `projectDirectoryAdded` 事件广播 `{ id }`，
//   监听方按 id 选中而非 path。

import { useState, useEffect, useCallback } from 'react';
import { Select, Form, Input, Modal, App } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import { getProjectDirectories, createProjectDirectory } from '@/utils/database/todos';

interface WorkspaceSelectProps {
  /** 当前选中的工作空间 ID（project_directories.id），唯一键 */
  value?: number | null;
  /** 变更回调，回传工作空间 ID（null 表示清空） */
  onChange?: (workspaceId: number | null) => void;
  /** 是否必填（影响 Select 的 allowClear） */
  required?: boolean;
  /** 是否显示下拉菜单底部"新建工作空间"选项，默认 true */
  showAddOption?: boolean;
  /** antd Select 原生 props 透传 */
  selectProps?: Record<string, unknown>;
}

interface QuickAddFormValues {
  name: string;
  path: string;
}

export function WorkspaceSelect({ value, onChange, required, showAddOption = true, selectProps }: WorkspaceSelectProps) {
  const { message } = App.useApp();
  // options.value 存 id（number），label 只展示 name，不在 UI 上暴露路径字符串。
  const [options, setOptions] = useState<{ label: string; value: number }[]>([]);
  const [loading, setLoading] = useState(true);
  const [quickAddOpen, setQuickAddOpen] = useState(false);
  const [quickAddSaving, setQuickAddSaving] = useState(false);
  const [quickAddForm] = Form.useForm<QuickAddFormValues>();

  // 加载目录列表：value 是 id，label 是用户可见的展示文本
  const loadDirs = useCallback(async () => {
    setLoading(true);
    try {
      const dirs = await getProjectDirectories();
      setOptions(dirs.map(d => ({
        // label 只展示 name，不在 UI 上暴露路径字符串；
        // ?? '' 确保类型为 string（d.name 是 string | null），满足 antd Select 的 label 类型要求
        label: d.name ?? '',
        value: d.id,
      })));
    } catch {
      message.error('加载工作空间列表失败');
    } finally {
      setLoading(false);
    }
  }, [message]);

  useEffect(() => { loadDirs(); }, [loadDirs]);

  // 监听外部 directory 新增事件：按 detail.id 选中，而不是 path。
  // 事件 payload 形如 { id: number }；跨组件刷新来源见 ProjectDirectoriesPanel / WorkspaceSwitcher。
  useEffect(() => {
    const handleDirAdded = (event: Event) => {
      const customEvent = event as CustomEvent<{ id: number }>;
      loadDirs().then(() => {
        if (customEvent.detail?.id != null) {
          onChange?.(customEvent.detail.id);
        }
      });
    };
    window.addEventListener('projectDirectoryAdded', handleDirAdded);
    return () => window.removeEventListener('projectDirectoryAdded', handleDirAdded);
  }, [loadDirs, onChange]);

  // 快捷新建：保存并使用
  // 后端 createProjectDirectory 现在返回 { id, path, name } 完整结构，按 id 通知父组件。
  const handleQuickAdd = useCallback(async () => {
    const values = await quickAddForm.validateFields();
    setQuickAddSaving(true);
    try {
      // createProjectDirectory(path, name) — 路径在前，名称在后；返回值含新建目录的 id
      const created = await createProjectDirectory(values.path.trim(), values.name.trim());
      message.success('工作空间已创建');
      quickAddForm.resetFields();
      setQuickAddOpen(false);
      // 刷新列表后选中新建项（按 id 选中）
      const dirs = await getProjectDirectories();
      setOptions(dirs.map(d => ({
        // label 只展示 name；?? '' 确保 label 类型为 string
        label: d.name ?? '',
        value: d.id,
      })));
      onChange?.(created.id);
      // 广播新增事件：payload 仅携带 id（破坏式），监听方按 id 刷新并选中。
      window.dispatchEvent(new CustomEvent('projectDirectoryAdded', { detail: { id: created.id } }));
    } catch (e) {
      message.error(`创建失败：${(e as Error).message}`);
    } finally {
      setQuickAddSaving(false);
    }
  }, [quickAddForm, message, onChange]);

  return (
    <>
      <Select
        value={value ?? undefined}
        onChange={onChange}
        options={options}
        loading={loading}
        placeholder="选择工作空间"
        showSearch={false}
        allowClear={!required}
        dropdownRender={(menu) => (
          <>
            {menu}
            {showAddOption && (
              <div
                style={{
                  padding: '4px 8px',
                  borderTop: '1px solid var(--color-border-secondary)',
                  cursor: 'pointer',
                  color: 'var(--color-primary)',
                  display: 'flex',
                  alignItems: 'center',
                  gap: 4,
                  fontSize: 13,
                }}
                onClick={(e) => {
                  e.stopPropagation();
                  setQuickAddOpen(true);
                }}
              >
                <PlusOutlined style={{ fontSize: 12 }} />
                新建工作空间
              </div>
            )}
          </>
        )}
        style={{ minWidth: 160 }}
        {...selectProps}
      />

      <Modal
        title="新建工作空间"
        open={quickAddOpen}
        onCancel={() => {
          if (quickAddSaving) return;
          quickAddForm.resetFields();
          setQuickAddOpen(false);
        }}
        onOk={handleQuickAdd}
        confirmLoading={quickAddSaving}
        okText="保存并使用"
        cancelText="取消"
        destroyOnClose
        maskClosable={!quickAddSaving}
      >
        <Form form={quickAddForm} layout="vertical" preserve={false}>
          <Form.Item
            label="工作空间名称"
            name="name"
            rules={[{ required: true, message: '请输入工作空间名称' }]}
          >
            <Input placeholder="例如：ntd 官网" autoFocus />
          </Form.Item>
          <Form.Item
            label="目录路径"
            name="path"
            rules={[{ required: true, message: '请输入目录路径' }]}
          >
            <Input placeholder="例如：/Users/me/projects/ntd-site" />
          </Form.Item>
        </Form>
      </Modal>
    </>
  );
}