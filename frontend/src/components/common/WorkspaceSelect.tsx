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

import { useState, useEffect, useCallback } from 'react';
import { Select, Form, Input, Button, Modal, Space, App } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import { getProjectDirectories, createProjectDirectory } from '@/utils/database/todos';

interface WorkspaceSelectProps {
  /** 当前选中的工作空间路径 */
  value?: string | null;
  /** 变更回调 */
  onChange?: (workspace: string | null) => void;
  /** 是否必填（影响 Select 的 allowClear） */
  required?: boolean;
  /** antd Select 原生 props 透传 */
  selectProps?: Record<string, unknown>;
}

interface QuickAddFormValues {
  name: string;
  path: string;
}

export function WorkspaceSelect({ value, onChange, required, selectProps }: WorkspaceSelectProps) {
  const { message } = App.useApp();
  const [options, setOptions] = useState<{ label: string; value: string }[]>([]);
  const [loading, setLoading] = useState(true);
  const [quickAddOpen, setQuickAddOpen] = useState(false);
  const [quickAddSaving, setQuickAddSaving] = useState(false);
  const [quickAddForm] = Form.useForm<QuickAddFormValues>();

  // 加载目录列表
  const loadDirs = useCallback(async () => {
    setLoading(true);
    try {
      const dirs = await getProjectDirectories();
      setOptions(dirs.map(d => ({
        label: d.name ? `${d.name}（${d.path}）` : d.path,
        value: d.path,
      })));
    } catch {
      message.error('加载工作空间列表失败');
    } finally {
      setLoading(false);
    }
  }, [message]);

  useEffect(() => { loadDirs(); }, [loadDirs]);

  // 监听外部 directory 新增事件，刷新列表并自动选中新目录
  useEffect(() => {
    const handleDirAdded = (event: Event) => {
      const customEvent = event as CustomEvent<{ path: string }>;
      loadDirs().then(() => {
        if (customEvent.detail?.path) {
          onChange?.(customEvent.detail.path);
        }
      });
    };
    window.addEventListener('projectDirectoryAdded', handleDirAdded);
    return () => window.removeEventListener('projectDirectoryAdded', handleDirAdded);
  }, [loadDirs, onChange]);

  // 快捷新建：保存并使用
  const handleQuickAdd = useCallback(async () => {
    const values = await quickAddForm.validateFields();
    setQuickAddSaving(true);
    try {
      // createProjectDirectory(path, name) — 路径在前，名称在后
      await createProjectDirectory(values.path.trim(), values.name.trim());
      message.success('工作空间已创建');
      quickAddForm.resetFields();
      setQuickAddOpen(false);
      // 刷新列表后选中新建项
      const dirs = await getProjectDirectories();
      setOptions(dirs.map(d => ({
        label: d.name ? `${d.name}（${d.path}）` : d.path,
        value: d.path,
      })));
      onChange?.(values.path.trim());
      // 广播新增事件，供 TodoList 等组件刷新分组数据
      window.dispatchEvent(new CustomEvent('projectDirectoryAdded', { detail: { path: values.path.trim() } }));
    } catch (e) {
      message.error(`创建失败：${(e as Error).message}`);
    } finally {
      setQuickAddSaving(false);
    }
  }, [quickAddForm, message, onChange]);

  return (
    <>
      <Space.Compact style={{ width: '100%' }}>
        <Select
          value={value}
          onChange={onChange}
          options={options}
          loading={loading}
          placeholder="选择工作空间"
          showSearch
          allowClear={!required}
          optionFilterProp="label"
          style={{ flex: 1 }}
          {...selectProps}
        />
        <Button
          icon={<PlusOutlined />}
          onClick={() => setQuickAddOpen(true)}
          title="新建工作空间"
          aria-label="新建工作空间"
        />
      </Space.Compact>

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
