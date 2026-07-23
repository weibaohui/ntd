//! 供应商池 + Profile 管理面板（支持移动端）。
//!
//! 两个 Tab：
//! - 供应商：管理 API Key / Base URL / 协议 / 模型列表
//! - Profile：从供应商选模型，配置各执行器

import { useState, useEffect, useCallback } from 'react';
import { Button, Card, Empty, Form, Input, List, message, Modal, Space, Spin, Table, Tag, Tabs, Typography, Popconfirm, Select, Switch, Flex, Alert, Tooltip } from 'antd';
import { PlusOutlined, KeyOutlined, SwapOutlined, DeleteOutlined, EditOutlined, DatabaseOutlined, ProfileOutlined, CheckCircleOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { useIsMobile } from '@/hooks/useIsMobile';

const { Text, Paragraph, Title } = Typography;

// ============================================================================
// API 路径
// ============================================================================
const PROVIDERS_API = '/api/v1/providers';
const PROFILES_API = '/api/v1/profiles';

// ============================================================================
// Type 定义
// ============================================================================

interface ProviderSummary {
  name: string;
  display_name: string;
  base_url: string;
  protocol: 'openai' | 'anthropic';
  supports_1m_context: boolean;
  model_count: number;
}

interface ProviderModel {
  name: string;
  display_name?: string;
}

interface ProviderDetail {
  name: string;
  display_name: string;
  api_key: string;
  base_url: string;
  protocol: 'openai' | 'anthropic';
  supports_1m_context: boolean;
  models: ProviderModel[];
}

interface ExecutorRef {
  provider: string;
  model: string;
}

interface ProfileSummary {
  name: string;
  display_name: string;
  description: string | null;
  executor_count: number;
  is_current: boolean;
}

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

async function fetchProviders(): Promise<ProviderSummary[]> {
  const r = await fetch(PROVIDERS_API);
  const j = await r.json();
  return j.data || [];
}

async function fetchProviderDetail(name: string): Promise<ProviderDetail | null> {
  try {
    // Get current profile to extract provider detail until we have GET /api/v1/providers/{name}
    // Actually let's use the profiles endpoint to look up provider info
    const r = await fetch(`${PROVIDERS_API}`);
    const j = await r.json();
    const providers: ProviderDetail[] = j.data || [];
    return providers.find((p: any) => p.name === name) || null;
  } catch { return null; }
}

async function createProvider(req: { name: string; display_name: string; api_key: string; base_url: string; protocol: string; supports_1m_context: boolean; models: ProviderModel[] }): Promise<void> {
  const r = await fetch(PROVIDERS_API, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(req) });
  if (!r.ok) { const e = await r.json().catch(() => ({ message: r.statusText })); throw new Error(e.message || '创建失败'); }
}

async function updateProvider(name: string, req: any): Promise<void> {
  const r = await fetch(`${PROVIDERS_API}/${encodeURIComponent(name)}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(req) });
  if (!r.ok) { const e = await r.json().catch(() => ({ message: r.statusText })); throw new Error(e.message || '更新失败'); }
}

async function deleteProvider(name: string): Promise<void> {
  const r = await fetch(`${PROVIDERS_API}/${encodeURIComponent(name)}`, { method: 'DELETE' });
  if (!r.ok) { const e = await r.json().catch(() => ({ message: r.statusText })); throw new Error(e.message || '删除失败'); }
}

async function fetchProfiles(): Promise<ProfileSummary[]> {
  const r = await fetch(PROFILES_API);
  const j = await r.json();
  return j.data || [];
}

async function createProfile(req: { name: string; display_name: string; description?: string; executors?: Record<string, ExecutorRef> }): Promise<void> {
  const r = await fetch(PROFILES_API, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(req) });
  if (!r.ok) { const e = await r.json().catch(() => ({ message: r.statusText })); throw new Error(e.message || '创建失败'); }
}

async function updateProfile(name: string, req: { display_name?: string; description?: string; executors?: Record<string, any> }): Promise<void> {
  const r = await fetch(`${PROFILES_API}/${encodeURIComponent(name)}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(req) });
  if (!r.ok) { const e = await r.json().catch(() => ({ message: r.statusText })); throw new Error(e.message || '更新失败'); }
}

async function deleteProfile(name: string): Promise<void> {
  const r = await fetch(`${PROFILES_API}/${encodeURIComponent(name)}`, { method: 'DELETE' });
  if (!r.ok) { const e = await r.json().catch(() => ({ message: r.statusText })); throw new Error(e.message || '删除失败'); }
}

async function applyProfile(name: string): Promise<ApplyProfileResult> {
  const r = await fetch(`${PROFILES_API}/${encodeURIComponent(name)}/apply`, { method: 'POST' });
  if (!r.ok) { const e = await r.json().catch(() => ({ message: r.statusText })); throw new Error(e.message || '应用失败'); }
  const j = await r.json();
  return j.data;
}

// ============================================================================
// 执行器常量
// ============================================================================
const EXECUTOR_OPTIONS = [
  { value: 'claudecode', label: 'Claude Code' },
  { value: 'pi', label: 'PI' },
  { value: 'atomcode', label: 'AtomCode' },
  { value: 'kilo', label: 'Kilo' },
  { value: 'codebuddy', label: 'CodeBuddy' },
  { value: 'opencode', label: 'Opencode' },
  { value: 'kimi', label: 'Kimi' },
  { value: 'mimo', label: 'MiMo' },
  { value: 'zhanlu', label: 'Zhanlu' },
];

// ============================================================================
// Provider 管理子组件
// ============================================================================

function ProviderManager({ isMobile }: { isMobile: boolean }) {
  const [providers, setProviders] = useState<ProviderSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [modalVisible, setModalVisible] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [form] = Form.useForm();
  const [modelList, setModelList] = useState<ProviderModel[]>([]);

  const load = useCallback(async () => {
    setLoading(true);
    try { setProviders(await fetchProviders()); }
    catch (err: any) { message.error('加载供应商失败: ' + (err?.message || String(err))); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => { load(); }, [load]);

  const openCreate = useCallback(() => {
    setEditing(null);
    form.resetFields();
    setModelList([]);
    setModalVisible(true);
  }, [form]);

  const openEdit = useCallback(async (name: string) => {
    setEditing(name);
    form.resetFields();
    setModelList([]);
    try {
      const r = await fetch(`${PROVIDERS_API}`);
      const j = await r.json();
      const all: any[] = j.data || [];
      // Get provider detail by re-fetching... For now use list data
      // We need GET /api/v1/providers/{name} but not yet implemented
      // Use a workaround: show detail in a separate view
      message.info('Provider 详情编辑功能待完善');
    } catch { /* ignore */ }
    setModalVisible(true);
  }, [form]);

  const handleSave = useCallback(async () => {
    try {
      const v = await form.validateFields();
      const models = modelList.filter(m => m.name.trim());
      const body = { ...v, models, supports_1m_context: v.supports_1m_context || false, protocol: v.protocol || 'openai' };
      if (editing) {
        await updateProvider(editing, body);
        message.success('供应商已更新');
      } else {
        await createProvider(body);
        message.success('供应商已创建');
      }
      setModalVisible(false);
      load();
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('操作失败: ' + (err?.message || String(err)));
    }
  }, [form, editing, modelList, load]);

  const handleDelete = useCallback(async (name: string) => {
    try { await deleteProvider(name); message.success('已删除'); load(); }
    catch (err: any) { message.error('删除失败: ' + (err?.message || String(err))); }
  }, [load]);

  const addModel = useCallback(() => {
    setModelList(prev => [...prev, { name: '', display_name: '' }]);
  }, []);

  const updateModel = useCallback((idx: number, field: 'name' | 'display_name', value: string) => {
    setModelList(prev => prev.map((m, i) => i === idx ? { ...m, [field]: value } : m));
  }, []);

  const removeModel = useCallback((idx: number) => {
    setModelList(prev => prev.filter((_, i) => i !== idx));
  }, []);

  // Provider Table columns
  const columns = [
    { title: '名称', dataIndex: 'display_name', key: 'name',
      render: (n: string, r: ProviderSummary) => (
        <Space>
          <Text strong>{n}</Text>
          <Tag color={r.protocol === 'anthropic' ? 'purple' : 'blue'}>{r.protocol === 'anthropic' ? 'Anthropic' : 'OpenAI'}</Tag>
          {r.supports_1m_context && <Tag color="orange">1M ctx</Tag>}
        </Space>
      )
    },
    { title: 'Base URL', dataIndex: 'base_url', key: 'base_url', ellipsis: true, width: 300 },
    { title: '模型数', dataIndex: 'model_count', key: 'model_count', width: 80 },
    { title: '操作', key: 'actions', width: 160,
      render: (_: any, r: ProviderSummary) => (
        <Space>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(r.name)} />
          <Popconfirm title="确认删除？" onConfirm={() => handleDelete(r.name)} okText="删除" cancelText="取消">
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      )
    },
  ];

  return (
    <>
      <PageCard
        icon={<DatabaseOutlined />}
        title="供应商"
        extra={
          isMobile ? (
            <Flex gap={8}>
              <Button type="primary" size="small" icon={<PlusOutlined />} onClick={openCreate}>新建</Button>
              <Button size="small" onClick={load} loading={loading}>刷新</Button>
            </Flex>
          ) : (
            <Space>
              <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>新建供应商</Button>
              <Button onClick={load} loading={loading}>刷新</Button>
            </Space>
          )
        }
      >
        {!isMobile && (
          <Paragraph type="secondary" style={{ marginBottom: 16 }}>
            集中管理各 AI 服务的 API Key、Base URL、协议格式和可用模型。
            供应商创建后，可以在 Profile 中引用。
          </Paragraph>
        )}
        <Spin spinning={loading}>
          {isMobile ? (
            <Card style={{ borderRadius: 8 }} bodyStyle={{ padding: '0 16px' }}>
              <List
                dataSource={providers}
                renderItem={(r) => (
                  <List.Item style={{ padding: '12px 0', borderBottom: '1px solid #f0f0f0' }}>
                    <Flex vertical gap={6} style={{ width: '100%' }}>
                      <Flex align="center" gap={4} wrap="wrap">
                        <Text strong>{r.display_name}</Text>
                        <Tag color={r.protocol === 'anthropic' ? 'purple' : 'blue'} style={{ fontSize: 11 }}>{r.protocol}</Tag>
                        {r.supports_1m_context && <Tag color="orange" style={{ fontSize: 11 }}>1M</Tag>}
                      </Flex>
                      <Text type="secondary" style={{ fontSize: 12, wordBreak: 'break-all' }}>{r.base_url}</Text>
                      <Text type="secondary" style={{ fontSize: 12 }}>{r.model_count} 个模型</Text>
                      <Flex gap={8}>
                        <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(r.name)} />
                        <Popconfirm title="确认删除？" onConfirm={() => handleDelete(r.name)} okText="删除" cancelText="取消">
                          <Button size="small" danger icon={<DeleteOutlined />} />
                        </Popconfirm>
                      </Flex>
                    </Flex>
                  </List.Item>
                )}
                locale={{ emptyText: <Empty description="暂无供应商" /> }}
              />
            </Card>
          ) : (
            <Table dataSource={providers} columns={columns} rowKey="name" pagination={false}
              locale={{ emptyText: <Empty description="暂无供应商，点击「新建供应商」添加" /> }}
            />
          )}
        </Spin>
      </PageCard>

      <Modal
        title={editing ? '编辑供应商' : '新建供应商'}
        open={modalVisible}
        onOk={handleSave}
        onCancel={() => setModalVisible(false)}
        okText={editing ? '保存' : '创建'}
        cancelText="取消"
        width={isMobile ? '100%' : 640}
        style={isMobile ? { top: 0, maxWidth: '100%', margin: 0 } : undefined}
      >
        <Form form={form} layout="vertical" size={isMobile ? 'middle' : undefined}>
          <Form.Item name="name" label="标识符" rules={[
            { required: true, message: '必填' },
            { pattern: /^[a-zA-Z0-9_-]+$/, message: '仅允许字母、数字、中划线、下划线' },
          ]}>
            <Input placeholder="如: deepseek-anthropic" disabled={!!editing} />
          </Form.Item>
          <Form.Item name="display_name" label="显示名称" rules={[{ required: true, message: '必填' }]}>
            <Input placeholder="如: DeepSeek (Anthropic 协议)" />
          </Form.Item>
          <Form.Item name="api_key" label="API Key" rules={[{ required: true, message: '必填' }]}>
            <Input.Password placeholder="sk-xxx" />
          </Form.Item>
          <Form.Item name="base_url" label="Base URL" rules={[{ required: true, message: '必填' }]}>
            <Input placeholder="https://api.example.com/v1" />
          </Form.Item>
          <Flex gap={16} wrap="wrap">
            <Form.Item name="protocol" label="协议格式" initialValue="openai" style={{ minWidth: 160 }}>
              <Select options={[
                { value: 'openai', label: 'OpenAI 兼容' },
                { value: 'anthropic', label: 'Anthropic 原生' },
              ]} />
            </Form.Item>
            <Form.Item name="supports_1m_context" label="1M 上下文" valuePropName="checked" initialValue={false}>
              <Switch />
            </Form.Item>
          </Flex>

          {/* 模型列表 */}
          <div style={{ marginTop: 16 }}>
            <Flex justify="space-between" align="center" style={{ marginBottom: 8 }}>
              <Text strong>模型列表</Text>
              <Button size="small" type="dashed" icon={<PlusOutlined />} onClick={addModel}>添加模型</Button>
            </Flex>
            {modelList.length === 0 && (
              <Text type="secondary">暂无模型，点击「添加模型」添加</Text>
            )}
            {modelList.map((m, i) => (
              <Flex key={i} gap={8} align="center" style={{ marginBottom: 8 }}>
                <Input size="small" placeholder="模型标识" value={m.name} onChange={e => updateModel(i, 'name', e.target.value)} style={{ flex: 1 }} />
                <Input size="small" placeholder="显示名称（可选）" value={m.display_name || ''} onChange={e => updateModel(i, 'display_name', e.target.value)} style={{ flex: 1 }} />
                <Button size="small" danger icon={<DeleteOutlined />} onClick={() => removeModel(i)} />
              </Flex>
            ))}
          </div>
        </Form>
      </Modal>
    </>
  );
}

// ============================================================================
// Profile 管理子组件
// ============================================================================

function ProfileManager({ isMobile }: { isMobile: boolean }) {
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [providerOptions, setProviderOptions] = useState<{ name: string; models: string[] }[]>([]);
  const [loading, setLoading] = useState(true);
  const [applyLoading, setApplyLoading] = useState<string | null>(null);
  const [createVisible, setCreateVisible] = useState(false);
  const [editVisible, setEditVisible] = useState(false);
  const [editProfileName, setEditProfileName] = useState<string | null>(null);
  const [resultVisible, setResultVisible] = useState<ApplyProfileResult | null>(null);
  const [createForm] = Form.useForm();
  const [editForm] = Form.useForm();

  // Profile 编辑：每个执行器的配置
  const [executorConfigs, setExecutorConfigs] = useState<Record<string, ExecutorRef>>({});

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [pfs, pvs] = await Promise.all([fetchProfiles(), fetchProviders()]);
      setProfiles(pfs);
      setProviderOptions(pvs.map(p => ({ name: p.name, models: [] })));
      // Fetch model lists for each provider
      pvs.forEach(pv => {
        fetch(PROVIDERS_API).then(r => r.json()).then(j => {
          const all: any[] = j.data || [];
          const detail = all.find((d: any) => d.name === pv.name);
          if (detail?.models) {
            setProviderOptions(prev => prev.map(po =>
              po.name === pv.name ? { ...po, models: detail.models.map((m: any) => m.name) } : po
            ));
          }
        }).catch(() => {});
      });
    } catch (err: any) { message.error('加载失败: ' + (err?.message || String(err))); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => { load(); }, [load]);

  const openCreate = useCallback(() => {
    createForm.resetFields();
    setCreateVisible(true);
  }, [createForm]);

  const handleCreate = useCallback(async () => {
    try {
      const v = await createForm.validateFields();
      await createProfile({ name: v.name, display_name: v.display_name, description: v.description });
      message.success('Profile 已创建');
      setCreateVisible(false);
      load();
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('创建失败: ' + (err?.message || String(err)));
    }
  }, [createForm, load]);

  const openEdit = useCallback(async (name: string) => {
    setEditProfileName(name);
    editForm.resetFields();
    setExecutorConfigs({});
    try {
      const r = await fetch(`${PROFILES_API}/current`);
      const j = await r.json();
      const detail = j.data;
      if (detail && detail.name === name && detail.executors) {
        setExecutorConfigs(detail.executors);
      }
    } catch { /* ignore */ }
    setEditVisible(true);
  }, [editForm]);

  const addExecutor = useCallback(() => {
    setExecutorConfigs(prev => {
      const key = `executor_${Object.keys(prev).length + 1}`;
      return { ...prev, [key]: { provider: '', model: '' } };
    });
  }, []);

  const updateExecutor = useCallback((key: string, field: 'provider' | 'model', value: string) => {
    setExecutorConfigs(prev => {
      const current = prev[key] || { provider: '', model: '' };
      return { ...prev, [key]: { ...current, [field]: value } };
    });
  }, []);

  const removeExecutor = useCallback((key: string) => {
    setExecutorConfigs(prev => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
  }, []);

  const handleEditSave = useCallback(async () => {
    if (!editProfileName) return;
    try {
      const v = await editForm.validateFields();
      // Build executors map from configs
      const executors: Record<string, ExecutorRef> = {};
      Object.entries(executorConfigs).forEach(([execName, ref]) => {
        if (execName && ref.provider && ref.model) {
          executors[execName] = ref;
        }
      });
      await updateProfile(editProfileName, { display_name: v.display_name, description: v.description, executors });
      message.success('保存成功');
      setEditVisible(false);
      load();
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('保存失败: ' + (err?.message || String(err)));
    }
  }, [editForm, editProfileName, executorConfigs, load]);

  const handleDelete = useCallback(async (name: string) => {
    try { await deleteProfile(name); message.success('已删除'); load(); }
    catch (err: any) { message.error('删除失败: ' + (err?.message || String(err))); }
  }, [load]);

  const handleApply = useCallback(async (name: string) => {
    setApplyLoading(name);
    try { const r = await applyProfile(name); setResultVisible(r); load(); }
    catch (err: any) { message.error('应用失败: ' + (err?.message || String(err))); }
    finally { setApplyLoading(null); }
  }, [load]);

  const getProviderModels = useCallback((providerName: string): string[] => {
    return providerOptions.find(p => p.name === providerName)?.models || [];
  }, [providerOptions]);

  // Table columns
  const columns = [
    { title: '状态', dataIndex: 'is_current', key: 'is_current', width: 80,
      render: (c: boolean) => c ? <Tag icon={<CheckCircleOutlined />} color="success">当前</Tag> : null
    },
    { title: '名称', dataIndex: 'display_name', key: 'name',
      render: (n: string, r: ProfileSummary) => (
        <Space>
          <Text strong>{n}</Text>
          <Text type="secondary">({r.name})</Text>
          {r.executor_count > 0 && <Tag color="blue">{r.executor_count} 个执行器</Tag>}
        </Space>
      )
    },
    { title: '描述', dataIndex: 'description', key: 'description', ellipsis: true,
      render: (d: string | null) => d || <Text type="secondary">-</Text>
    },
    { title: '操作', key: 'actions', width: 320,
      render: (_: any, r: ProfileSummary) => (
        <Space>
          <Button type="primary" size="small" icon={<SwapOutlined />}
            loading={applyLoading === r.name} disabled={r.is_current}
            onClick={() => handleApply(r.name)}>{r.is_current ? '已激活' : '切换'}</Button>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(r.name)} />
          <Popconfirm title="确认删除？" description="当前激活的不可删除" onConfirm={() => handleDelete(r.name)} okText="删除" cancelText="取消">
            <Button size="small" danger icon={<DeleteOutlined />} disabled={r.is_current} />
          </Popconfirm>
        </Space>
      )
    },
  ];

  return (
    <>
      <PageCard
        icon={<ProfileOutlined />}
        title="Profile"
        extra={
          isMobile ? (
            <Flex gap={8}>
              <Button type="primary" size="small" icon={<PlusOutlined />} onClick={openCreate}>新建</Button>
              <Button size="small" onClick={load} loading={loading}>刷新</Button>
            </Flex>
          ) : (
            <Space>
              <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>新建 Profile</Button>
              <Button onClick={load} loading={loading}>刷新</Button>
            </Space>
          )
        }
      >
        {!isMobile && (
          <Paragraph type="secondary" style={{ marginBottom: 16 }}>
            为每个执行器选择供应商和模型。应用 Profile 时将自动从供应商获取 API Key，
            按各执行器的配置文件格式写入。
          </Paragraph>
        )}
        <Spin spinning={loading}>
          {isMobile ? (
            <Card style={{ borderRadius: 8 }} bodyStyle={{ padding: '0 16px' }}>
              <List dataSource={profiles}
                renderItem={(r) => (
                  <List.Item style={{ padding: '12px 0', borderBottom: '1px solid #f0f0f0' }}>
                    <Flex vertical gap={8} style={{ width: '100%' }}>
                      <Flex align="center" gap={4} wrap="wrap">
                        {r.is_current && <Tag icon={<CheckCircleOutlined />} color="success" style={{ marginRight: 0, fontSize: 11 }}>当前</Tag>}
                        <Text strong style={{ fontSize: 15 }}>{r.display_name}</Text>
                        <Text type="secondary" style={{ fontSize: 12 }}>({r.name})</Text>
                        {r.executor_count > 0 && <Tag color="blue" style={{ fontSize: 11 }}>{r.executor_count}个</Tag>}
                      </Flex>
                      {r.description && <Text type="secondary" style={{ fontSize: 13 }}>{r.description}</Text>}
                      <Flex gap={8}>
                        <Button type="primary" size="small" icon={<SwapOutlined />}
                          loading={applyLoading === r.name} disabled={r.is_current}
                          onClick={() => handleApply(r.name)}>{r.is_current ? '已激活' : '切换'}</Button>
                        <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(r.name)} />
                        <Popconfirm title="确认删除？" onConfirm={() => handleDelete(r.name)} okText="删除" cancelText="取消">
                          <Button size="small" danger icon={<DeleteOutlined />} disabled={r.is_current} />
                        </Popconfirm>
                      </Flex>
                    </Flex>
                  </List.Item>
                )}
                locale={{ emptyText: <Empty description="暂无 Profile" /> }}
              />
            </Card>
          ) : (
            <Table dataSource={profiles} columns={columns} rowKey="name" pagination={false}
              locale={{ emptyText: <Empty description="暂无 Profile，点击「新建 Profile」创建" /> }}
            />
          )}
        </Spin>
      </PageCard>

      {/* 创建 Profile 弹窗 */}
      <Modal title="新建 Profile" open={createVisible} onOk={handleCreate} onCancel={() => setCreateVisible(false)}
        okText="创建" cancelText="取消" width={isMobile ? '100%' : 520}
        style={isMobile ? { top: 0, maxWidth: '100%', margin: 0 } : undefined}>
        <Form form={createForm} layout="vertical" size={isMobile ? 'middle' : undefined}>
          <Form.Item name="name" label="标识符" rules={[
            { required: true, message: '必填' },
            { pattern: /^[a-zA-Z0-9_-]+$/, message: '仅允许字母、数字、中划线、下划线' },
          ]}>
            <Input placeholder="如: daily-dev" />
          </Form.Item>
          <Form.Item name="display_name" label="显示名称" rules={[{ required: true, message: '必填' }]}>
            <Input placeholder="如: 日常开发" />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={2} placeholder="可选描述" />
          </Form.Item>
        </Form>
      </Modal>

      {/* 编辑 Profile 弹窗 */}
      <Modal title="编辑 Profile 执行器配置" open={editVisible}
        onOk={handleEditSave} onCancel={() => setEditVisible(false)}
        okText="保存" cancelText="取消"
        width={isMobile ? '100%' : 640}
        style={isMobile ? { top: 0, maxWidth: '100%', margin: 0 } : undefined}
      >
        <Form form={editForm} layout="vertical" initialValues={{ display_name: '', description: '' }} size={isMobile ? 'middle' : undefined}>
          <Form.Item name="display_name" label="显示名称" rules={[{ required: true, message: '必填' }]}>
            <Input />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={2} />
          </Form.Item>
        </Form>

        <div style={{ marginTop: 16 }}>
          <Flex justify="space-between" align="center" style={{ marginBottom: 8 }}>
            <Text strong>执行器配置（选择供应商 → 模型）</Text>
            <Button size="small" type="dashed" icon={<PlusOutlined />} onClick={addExecutor}>添加执行器</Button>
          </Flex>

          {Object.keys(executorConfigs).length === 0 && (
            <Alert type="info" message="暂无执行器配置" description="点击「添加执行器」为 Profile 配置执行器" showIcon style={{ marginBottom: 16 }} />
          )}

          {Object.entries(executorConfigs).map(([execKey, ref]) => (
            <Card key={execKey} size="small" style={{ marginBottom: 8 }}>
              <Flex vertical gap={8}>
                <Flex gap={8} align="center" wrap="wrap">
                  <Select
                    showSearch
                    style={{ minWidth: 140 }}
                    placeholder="选择执行器"
                    value={EXECUTOR_OPTIONS.find(o => o.value === execKey) ? execKey : execKey}
                    onChange={(v) => {
                      // Rename key - remove old, add new
                      const configs = { ...executorConfigs };
                      delete configs[execKey];
                      configs[v] = ref;
                      setExecutorConfigs(configs);
                    }}
                    options={EXECUTOR_OPTIONS}
                  />
                  <Select
                    showSearch
                    style={{ minWidth: 160 }}
                    placeholder="选择供应商"
                    value={ref.provider || undefined}
                    onChange={(v) => updateExecutor(execKey, 'provider', v)}
                    options={providerOptions.map(p => ({ value: p.name, label: p.name }))}
                  />
                  <Select
                    showSearch
                    style={{ minWidth: 160 }}
                    placeholder="选择模型"
                    value={ref.model || undefined}
                    onChange={(v) => updateExecutor(execKey, 'model', v)}
                    options={getProviderModels(ref.provider).map(m => ({ value: m, label: m }))}
                    disabled={!ref.provider}
                  />
                  <Button size="small" danger icon={<DeleteOutlined />} onClick={() => removeExecutor(execKey)} />
                </Flex>
              </Flex>
            </Card>
          ))}
        </div>
      </Modal>

      {/* 应用结果弹窗 */}
      <Modal title="Profile 已应用" open={!!resultVisible}
        onCancel={() => setResultVisible(null)}
        footer={<Button type="primary" onClick={() => setResultVisible(null)}>确定</Button>}
        width={isMobile ? '100%' : 520}
        style={isMobile ? { top: 0, maxWidth: '100%', margin: 0 } : undefined}
      >
        {resultVisible && (
          <div>
            <Alert type={resultVisible.errors.length > 0 ? 'warning' : 'success'}
              message={`Profile "${resultVisible.profile_display_name}" 已切换`}
              description={resultVisible.errors.length > 0
                ? `成功: ${resultVisible.applied_executors.length} 个，跳过: ${resultVisible.skipped_executors.length} 个，失败: ${resultVisible.errors.length} 个`
                : `已为 ${resultVisible.applied_executors.length} 个执行器写入配置文件`}
              showIcon style={{ marginBottom: 16 }}
            />
            {resultVisible.applied_executors.length > 0 && (
              <div style={{ marginBottom: 8 }}>
                <Text strong>已应用：</Text>
                <div style={{ marginTop: 4 }}>{resultVisible.applied_executors.map(n => <Tag key={n} color="success">{n}</Tag>)}</div>
              </div>
            )}
            {resultVisible.errors.length > 0 && (
              <div><Text strong type="danger">错误：</Text>
                <ul style={{ marginTop: 4, paddingLeft: 20 }}>
                  {resultVisible.errors.map((e, i) => <li key={i}><Text type="danger">{e}</Text></li>)}
                </ul>
              </div>
            )}
          </div>
        )}
      </Modal>
    </>
  );
}

// ============================================================================
// 主面板：两个 Tab
// ============================================================================

export function ProfilesPanel() {
  const isMobile = useIsMobile();

  const tabItems = [
    {
      key: 'providers',
      label: <span><DatabaseOutlined style={{ marginRight: 4 }} />供应商</span>,
      children: <ProviderManager isMobile={isMobile} />,
    },
    {
      key: 'profiles',
      label: <span><ProfileOutlined style={{ marginRight: 4 }} />Profile</span>,
      children: <ProfileManager isMobile={isMobile} />,
    },
  ];

  return (
    <Tabs
      defaultActiveKey="providers"
      items={tabItems}
      tabPosition={isMobile ? 'top' : 'left'}
      size={isMobile ? 'small' : 'middle'}
      style={{ minHeight: 200 }}
    />
  );
}
