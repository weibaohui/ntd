//! API Key Profile 管理面板。
//!
//! 统一管理各执行器的 API Key 和模型配置，支持多 Profile 切换。
//!
//! # 功能
//!
//! - 查看所有 Profile 列表
//! - 创建/编辑/删除 Profile
//! - 应用（切换）Profile → 写入各执行器配置文件
//! - 显示当前激活的 Profile

import { useState, useEffect, useCallback } from 'react';
import { Button, Card, Empty, Form, Input, message, Modal, Space, Spin, Table, Tag, Typography, Popconfirm, Descriptions, Alert } from 'antd';
import {
  PlusOutlined,
  KeyOutlined,
  CheckCircleOutlined,
  SwapOutlined,
  DeleteOutlined,
  EditOutlined,
} from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';

const { Text, Paragraph } = Typography;

/** Profile API 路径前缀 */
const PROFILES_API = '/api/v1/profiles';

// ============================================================================
// Type 定义
// ============================================================================

/** 前端使用的 Profile 摘要 */
interface ProfileSummary {
  name: string;
  display_name: string;
  description: string | null;
  executor_count: number;
  is_current: boolean;
}

/** Profile 详情（编辑弹窗中展示当前执行器配置用） */
interface ProfileDetail {
  name: string;
  display_name: string;
  description: string | null;
  executors: Record<string, ExecutorSettings>;
}

/** 执行器设置 */
interface ExecutorSettings {
  api_key?: string;
  base_url?: string;
  model?: string;
  [key: string]: string | undefined;
}

/** 应用 Profile 的结果 */
interface ApplyProfileResult {
  profile_name: string;
  profile_display_name: string;
  applied_executors: string[];
  skipped_executors: string[];
  errors: string[];
}

// ============================================================================
// API 调用
// ============================================================================

async function fetchProfiles(): Promise<ProfileSummary[]> {
  const resp = await fetch(PROFILES_API);
  const json = await resp.json();
  return json.data || [];
}

async function createProfile(req: { name: string; display_name: string; description?: string; executors?: Record<string, any> }): Promise<void> {
  const resp = await fetch(PROFILES_API, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ...req, executors: req.executors || {} }),
  });
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ message: resp.statusText }));
    throw new Error(err.message || '创建失败');
  }
}

async function updateProfile(name: string, req: { display_name?: string; description?: string; executors?: Record<string, any> }): Promise<void> {
  const resp = await fetch(`${PROFILES_API}/${encodeURIComponent(name)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  });
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ message: resp.statusText }));
    throw new Error(err.message || '更新失败');
  }
}

async function deleteProfile(name: string): Promise<void> {
  const resp = await fetch(`${PROFILES_API}/${encodeURIComponent(name)}`, {
    method: 'DELETE',
  });
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ message: resp.statusText }));
    throw new Error(err.message || '删除失败');
  }
}

async function applyProfile(name: string): Promise<ApplyProfileResult> {
  const resp = await fetch(`${PROFILES_API}/${encodeURIComponent(name)}/apply`, {
    method: 'POST',
  });
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ message: resp.statusText }));
    throw new Error(err.message || '应用失败');
  }
  const json = await resp.json();
  return json.data;
}

// ============================================================================
// 组件
// ============================================================================

/** API Key Profile 管理面板 */
export function ProfilesPanel() {
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [applyLoading, setApplyLoading] = useState<string | null>(null);
  const [modalVisible, setModalVisible] = useState(false);
  const [editModalVisible, setEditModalVisible] = useState(false);
  const [currentDetail, setCurrentDetail] = useState<ProfileDetail | null>(null);
  const [editProfileName, setEditProfileName] = useState<string | null>(null);
  const [resultVisible, setResultVisible] = useState<ApplyProfileResult | null>(null);
  const [form] = Form.useForm();
  const [editForm] = Form.useForm();

  const loadProfiles = useCallback(async () => {
    setLoading(true);
    try {
      const data = await fetchProfiles();
      setProfiles(data);
    } catch (err: any) {
      message.error('加载 Profile 列表失败: ' + (err?.message || String(err)));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadProfiles();
  }, [loadProfiles]);

  /** 打开新建 Profile 弹窗 */
  const handleOpenCreate = useCallback(() => {
    form.resetFields();
    form.setFieldsValue({ name: '', display_name: '' });
    setModalVisible(true);
  }, [form]);

  /** 创建 Profile */
  const handleCreate = useCallback(async () => {
    try {
      const values = await form.validateFields();
      await createProfile({
        name: values.name,
        display_name: values.display_name,
        description: values.description,
      });
      message.success(`Profile "${values.display_name}" 创建成功`);
      setModalVisible(false);
      loadProfiles();
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('创建失败: ' + (err?.message || String(err)));
    }
  }, [form, loadProfiles]);

  /** 打开编辑 Profile 弹窗 */
  const handleOpenEdit = useCallback(async (name: string) => {
    setEditProfileName(name);
    editForm.resetFields();
    try {
      const resp = await fetch(`${PROFILES_API}/current`);
      const json = await resp.json();
      const detail = json.data;
      if (detail && detail.name === name) {
        setCurrentDetail(detail);
      } else {
        // 如果不是当前 profile，从 profiles 列表显示基本信息
        setCurrentDetail(null);
      }
    } catch {
      setCurrentDetail(null);
    }
    setEditModalVisible(true);
  }, [editForm]);

  /** 编辑 Profile 基本信息 */
  const handleEdit = useCallback(async () => {
    if (!editProfileName) return;
    try {
      const values = await editForm.validateFields();
      await updateProfile(editProfileName, {
        display_name: values.display_name,
        description: values.description,
      });
      message.success('更新成功');
      setEditModalVisible(false);
      loadProfiles();
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('更新失败: ' + (err?.message || String(err)));
    }
  }, [editForm, editProfileName, loadProfiles]);

  /** 删除 Profile */
  const handleDelete = useCallback(async (name: string) => {
    try {
      await deleteProfile(name);
      message.success(`Profile "${name}" 已删除`);
      loadProfiles();
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  }, [loadProfiles]);

  /** 应用（切换）Profile */
  const handleApply = useCallback(async (name: string) => {
    setApplyLoading(name);
    try {
      const result = await applyProfile(name);
      setResultVisible(result);
      loadProfiles();
    } catch (err: any) {
      message.error('应用失败: ' + (err?.message || String(err)));
    } finally {
      setApplyLoading(null);
    }
  }, [loadProfiles]);

  /** 表格列定义 */
  const columns = [
    {
      title: '状态',
      dataIndex: 'is_current',
      key: 'is_current',
      width: 80,
      render: (is_current: boolean) =>
        is_current ? (
          <Tag icon={<CheckCircleOutlined />} color="success">当前</Tag>
        ) : null,
    },
    {
      title: '名称',
      dataIndex: 'display_name',
      key: 'display_name',
      render: (name: string, record: ProfileSummary) => (
        <Space>
          <Text strong>{name}</Text>
          <Text type="secondary">({record.name})</Text>
          {record.executor_count > 0 && (
            <Tag color="blue">{record.executor_count} 个执行器</Tag>
          )}
        </Space>
      ),
    },
    {
      title: '描述',
      dataIndex: 'description',
      key: 'description',
      ellipsis: true,
      render: (desc: string | null) => desc || <Text type="secondary">-</Text>,
    },
    {
      title: '操作',
      key: 'actions',
      width: 320,
      render: (_: any, record: ProfileSummary) => (
        <Space>
          <Button
            type="primary"
            size="small"
            icon={<SwapOutlined />}
            loading={applyLoading === record.name}
            disabled={record.is_current}
            onClick={() => handleApply(record.name)}
          >
            {record.is_current ? '已激活' : '切换'}
          </Button>
          <Button
            size="small"
            icon={<EditOutlined />}
            onClick={() => handleOpenEdit(record.name)}
          />
          <Popconfirm
            title="确认删除此 Profile？"
            description="当前激活的 Profile 不可删除。"
            onConfirm={() => handleDelete(record.name)}
            okText="删除"
            cancelText="取消"
          >
            <Button
              size="small"
              danger
              icon={<DeleteOutlined />}
              disabled={record.is_current}
            />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <>
      <PageCard
        icon={<KeyOutlined />}
        title="API Key 管理"
        extra={
          <Space>
            <Button type="primary" icon={<PlusOutlined />} onClick={handleOpenCreate}>
              新建 Profile
            </Button>
            <Button onClick={loadProfiles} loading={loading}>
              刷新
            </Button>
          </Space>
        }
      >
        <Paragraph type="secondary" style={{ marginBottom: 16 }}>
          统一管理各 AI 执行器（Claude Code、PI、AtomCode 等）的 API Key 和模型配置。
          通过 Profile 切换可以一键更换整套配置，适用于不同项目使用不同 API Key / 模型的场景。
        </Paragraph>

        <Spin spinning={loading}>
          <Table
            dataSource={profiles}
            columns={columns}
            rowKey="name"
            pagination={false}
            locale={{ emptyText: <Empty description="暂无 Profile，点击「新建 Profile」创建" /> }}
          />
        </Spin>
      </PageCard>

      {/* 新建 Profile 弹窗 */}
      <Modal
        title="新建 Profile"
        open={modalVisible}
        onOk={handleCreate}
        onCancel={() => setModalVisible(false)}
        okText="创建"
        cancelText="取消"
      >
        <Form form={form} layout="vertical">
          <Form.Item
            name="name"
            label="标识符"
            rules={[
              { required: true, message: '请输入标识符' },
              { pattern: /^[a-zA-Z0-9_-]+$/, message: '只能包含字母、数字、中划线、下划线' },
            ]}
          >
            <Input placeholder="如: work-profile" />
          </Form.Item>
          <Form.Item
            name="display_name"
            label="显示名称"
            rules={[{ required: true, message: '请输入显示名称' }]}
          >
            <Input placeholder="如: 工作配置" />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={2} placeholder="可选描述" />
          </Form.Item>
        </Form>
      </Modal>

      {/* 编辑 Profile 弹窗 */}
      <Modal
        title="编辑 Profile"
        open={editModalVisible}
        onOk={handleEdit}
        onCancel={() => setEditModalVisible(false)}
        okText="保存"
        cancelText="取消"
      >
        <Form form={editForm} layout="vertical" initialValues={{ display_name: '', description: '' }}>
          <Form.Item
            name="display_name"
            label="显示名称"
            rules={[{ required: true, message: '请输入显示名称' }]}
          >
            <Input />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={2} />
          </Form.Item>
        </Form>
        {currentDetail && (
          <Card title="当前执行器配置" size="small" style={{ marginTop: 16 }}>
            {Object.entries(currentDetail.executors).length === 0 ? (
              <Text type="secondary">暂未配置执行器</Text>
            ) : (
              <Descriptions column={1} size="small">
                {Object.entries(currentDetail.executors).map(([execName, settings]) => (
                  <Descriptions.Item key={execName} label={execName}>
                    {settings.api_key ? `API Key: ${maskKey(settings.api_key)}` : '未配置 API Key'}
                    {settings.model && ` | Model: ${settings.model}`}
                  </Descriptions.Item>
                ))}
              </Descriptions>
            )}
          </Card>
        )}
      </Modal>

      {/* 应用结果弹窗 */}
      <Modal
        title="Profile 已应用"
        open={!!resultVisible}
        onCancel={() => setResultVisible(null)}
        footer={<Button type="primary" onClick={() => setResultVisible(null)}>确定</Button>}
      >
        {resultVisible && (
          <div>
            <Alert
              type={resultVisible.errors.length > 0 ? 'warning' : 'success'}
              message={`Profile "${resultVisible.profile_display_name}" 已切换`}
              description={
                resultVisible.errors.length > 0
                  ? `成功: ${resultVisible.applied_executors.length} 个执行器，跳过: ${resultVisible.skipped_executors.length} 个，失败: ${resultVisible.errors.length} 个`
                  : `已为 ${resultVisible.applied_executors.length} 个执行器写入配置文件`
              }
              showIcon
              style={{ marginBottom: 16 }}
            />

            {resultVisible.applied_executors.length > 0 && (
              <div style={{ marginBottom: 8 }}>
                <Text strong>已应用的执行器：</Text>
                <div>
                  {resultVisible.applied_executors.map((name) => (
                    <Tag key={name} color="success">{name}</Tag>
                  ))}
                </div>
              </div>
            )}

            {resultVisible.skipped_executors.length > 0 && (
              <div style={{ marginBottom: 8 }}>
                <Text strong>跳过的执行器（暂无配置生成器）：</Text>
                <div>
                  {resultVisible.skipped_executors.map((name) => (
                    <Tag key={name}>{name}</Tag>
                  ))}
                </div>
              </div>
            )}

            {resultVisible.errors.length > 0 && (
              <div>
                <Text strong type="danger">错误：</Text>
                <ul>
                  {resultVisible.errors.map((err, i) => (
                    <li key={i}><Text type="danger">{err}</Text></li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
      </Modal>
    </>
  );
}

/** 将 API Key 中间部分替换为 `*`，只保留前后各 4 位 */
function maskKey(key: string): string {
  if (key.length <= 8) return '****';
  return key.slice(0, 4) + '****' + key.slice(-4);
}
