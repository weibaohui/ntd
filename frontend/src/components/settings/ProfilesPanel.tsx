//! 供应商管理面板。
//!
//! 每个 API Key 一张卡片，点击「应用」选择执行器，按执行器格式写入配置文件。

import { useState, useEffect, useCallback } from 'react';
import { Button, Card, Checkbox, Empty, Form, Input, List, message, Modal, Space, Spin, Tag, Typography, Flex, Alert, Divider, Row, Col } from 'antd';
import { PlusOutlined, SwapOutlined, DeleteOutlined, EditOutlined, DatabaseOutlined, CheckCircleOutlined, CloseCircleOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { useIsMobile } from '@/hooks/useIsMobile';

const { Text, Paragraph, Title } = Typography;

const PROVIDERS_API = '/api/v1/providers';

// ============================================================================
// Types
// ============================================================================

interface ProviderModel {
  name: string;
  display_name?: string;
  supports_1m_context?: boolean;
}

interface ProviderDetail {
  name: string;
  display_name: string;
  api_key: string;
  base_url: string;
  protocol: 'openai' | 'anthropic';
  models: ProviderModel[];
}

interface ApplyResult {
  applied: string[];
  errors: string[];
}

// 所有执行器列表
const ALL_EXECUTORS = [
  { value: 'claudecode', label: 'Claude Code' },
  { value: 'pi', label: 'PI' },
  { value: 'atomcode', label: 'AtomCode' },
  { value: 'kilo', label: 'Kilo' },
  { value: 'codebuddy', label: 'CodeBuddy' },
  { value: 'opencode', label: 'Opencode' },
  { value: 'kimi', label: 'Kimi' },
  { value: 'mimo', label: 'MiMo' },
  { value: 'zhanlu', label: 'Zhanlu' },
  { value: 'hermes', label: 'Hermes' },
  { value: 'codewhale', label: 'CodeWhale' },
  { value: 'codex', label: 'Codex' },
  { value: 'mobilecoder', label: 'MobileCoder' },
];

// ============================================================================
// Component
// ============================================================================

export function ProfilesPanel() {
  const isMobile = useIsMobile();
  const [providers, setProviders] = useState<ProviderDetail[]>([]);
  const [loading, setLoading] = useState(true);
  const [createVisible, setCreateVisible] = useState(false);
  const [editVisible, setEditVisible] = useState(false);
  const [editingName, setEditingName] = useState<string | null>(null);
  const [applyVisible, setApplyVisible] = useState(false);
  const [applyProvider, setApplyProvider] = useState<ProviderDetail | null>(null);
  const [selectedExecutors, setSelectedExecutors] = useState<string[]>([]);
  const [applying, setApplying] = useState(false);
  const [applyResult, setApplyResult] = useState<ApplyResult | null>(null);
  const [modelList, setModelList] = useState<ProviderModel[]>([]);
  const [form] = Form.useForm();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const r = await fetch(PROVIDERS_API);
      const j = await r.json();
      const list: any[] = j.data || [];
      // 加载每个 provider 的详情（含 api_key）
      const details = await Promise.all(
        list.map(async (s) => {
          try {
            const dr = await fetch(`${PROVIDERS_API}/${encodeURIComponent(s.name)}`);
            const dj = await dr.json();
            return dj.data;
          } catch { return null; }
        })
      );
      setProviders(details.filter(Boolean));
    } catch (err: any) {
      message.error('加载失败: ' + (err?.message || String(err)));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  // 打开新建
  const openCreate = useCallback(() => {
    setEditingName(null);
    form.resetFields();
    setModelList([]);
    setCreateVisible(true);
  }, [form]);

  // 打开编辑
  const openEdit = useCallback(async (name: string) => {
    setEditingName(name);
    form.resetFields();
    setModelList([]);
    try {
      const r = await fetch(`${PROVIDERS_API}/${encodeURIComponent(name)}`);
      if (!r.ok) { message.error('获取详情失败'); return; }
      const j = await r.json();
      const d = j.data;
      if (d) {
        form.setFieldsValue({
          name: d.name,
          display_name: d.display_name,
          api_key: d.api_key,
          base_url: d.base_url,
          protocol: d.protocol,
        });
        setModelList(d.models || []);
      }
    } catch (err: any) {
      message.error('加载失败: ' + (err?.message || String(err)));
    }
    setEditVisible(true);
  }, [form]);

  // 保存
  const handleSave = useCallback(async () => {
    try {
      const v = await form.validateFields();
      const models = modelList.filter((m: any) => m.name?.trim());
      const body = { ...v, models, protocol: v.protocol || 'openai' };
      if (editingName) {
        await fetch(`${PROVIDERS_API}/${encodeURIComponent(editingName)}`, {
          method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
        });
        message.success('已更新');
      } else {
        await fetch(PROVIDERS_API, {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
        });
        message.success('已创建');
      }
      setCreateVisible(false);
      setEditVisible(false);
      load();
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('操作失败: ' + (err?.message || String(err)));
    }
  }, [form, editingName, modelList, load]);

  // 删除
  const handleDelete = useCallback(async (name: string) => {
    try {
      await fetch(`${PROVIDERS_API}/${encodeURIComponent(name)}`, { method: 'DELETE' });
      message.success('已删除');
      load();
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  }, [load]);

  // 打开应用弹窗
  const openApply = useCallback((p: ProviderDetail) => {
    setApplyProvider(p);
    setSelectedExecutors([]);
    setApplyResult(null);
    setApplyVisible(true);
  }, []);

  // 执行应用
  const handleApply = useCallback(async () => {
    if (!applyProvider || selectedExecutors.length === 0) return;
    setApplying(true);
    try {
      const r = await fetch(`${PROVIDERS_API}/${encodeURIComponent(applyProvider.name)}/apply`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ executors: selectedExecutors }),
      });
      const j = await r.json();
      setApplyResult(j.data);
    } catch (err: any) {
      message.error('应用失败: ' + (err?.message || String(err)));
    } finally {
      setApplying(false);
    }
  }, [applyProvider, selectedExecutors]);

  // 模型列表操作
  const addModel = () => setModelList(prev => [...prev, { name: '', display_name: '' }]);
  const updateModel = (idx: number, field: string, value: any) =>
    setModelList(prev => prev.map((m, i) => i === idx ? { ...m, [field]: value } : m));
  const removeModel = (idx: number) => setModelList(prev => prev.filter((_, i) => i !== idx));

  // Provider 表单弹窗
  const formModal = (visible: boolean, onClose: () => void, title: string) => (
    <Modal title={title} open={visible} onOk={handleSave} onCancel={onClose}
      okText="保存" cancelText="取消"
      width={isMobile ? '100%' : 640}
      style={isMobile ? { top: 0, maxWidth: '100%', margin: 0 } : undefined}
    >
      <Form form={form} layout="vertical" size={isMobile ? 'middle' : undefined}>
        <Form.Item name="name" label="标识符" rules={[
          { required: true }, { pattern: /^[a-zA-Z0-9_-]+$/, message: '仅允许字母、数字、中划线、下划线' },
        ]}>
          <Input placeholder="如: deepseek-anthropic" disabled={!!editingName} />
        </Form.Item>
        <Form.Item name="display_name" label="显示名称" rules={[{ required: true }]}>
          <Input placeholder="如: DeepSeek" />
        </Form.Item>
        <Form.Item name="api_key" label="API Key" rules={[{ required: true }]}>
          <Input.Password placeholder="sk-xxx" />
        </Form.Item>
        <Form.Item name="base_url" label="Base URL" rules={[{ required: true }]}>
          <Input placeholder="https://api.example.com/v1" />
        </Form.Item>
        <Form.Item name="protocol" label="协议格式" initialValue="openai" style={{ maxWidth: 200 }}>
          <Input placeholder="openai / anthropic" />
        </Form.Item>

        <div style={{ marginTop: 16 }}>
          <Flex justify="space-between" align="center" style={{ marginBottom: 8 }}>
            <Text strong>模型列表</Text>
            <Button size="small" type="dashed" icon={<PlusOutlined />} onClick={addModel}>添加</Button>
          </Flex>
          {modelList.length === 0 && <Text type="secondary">暂无模型</Text>}
          {modelList.map((m, i) => (
            <Flex key={i} gap={8} align="center" style={{ marginBottom: 8 }} wrap="wrap">
              <Input size="small" placeholder="模型标识" value={m.name}
                onChange={e => updateModel(i, 'name', e.target.value)}
                style={{ flex: 1, minWidth: 100 }} />
              <Input size="small" placeholder="显示名称（可选）" value={m.display_name || ''}
                onChange={e => updateModel(i, 'display_name', e.target.value)}
                style={{ flex: 1, minWidth: 100 }} />
              <Checkbox checked={!!(m as any).supports_1m_context}
                onChange={e => updateModel(i, 'supports_1m_context', e.target.checked)}>
                <Text type="secondary" style={{ fontSize: 12 }}>1M</Text>
              </Checkbox>
              <Button size="small" danger icon={<DeleteOutlined />} onClick={() => removeModel(i)} />
            </Flex>
          ))}
        </div>
      </Form>
    </Modal>
  );

  return (
    <>
      <PageCard
        icon={<DatabaseOutlined />}
        title="API Key 管理"
        extra={
          isMobile ? (
            <Flex gap={8}>
              <Button type="primary" size="small" icon={<PlusOutlined />} onClick={openCreate}>新增</Button>
              <Button size="small" onClick={load} loading={loading}>刷新</Button>
            </Flex>
          ) : (
            <Space>
              <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>新增 API Key</Button>
              <Button onClick={load} loading={loading}>刷新</Button>
            </Space>
          )
        }
      >
        {!isMobile && (
          <Paragraph type="secondary" style={{ marginBottom: 16 }}>
            集中管理各 AI 服务的 API Key 和模型。点击「应用」选择执行器，按该执行器的格式写入配置文件。
          </Paragraph>
        )}

        <Spin spinning={loading}>
          {providers.length === 0 && !loading && (
            <Empty description="暂无 API Key，点击「新增 API Key」添加" />
          )}

          <Row gutter={[16, 16]}>
            {providers.map(p => (
              <Col xs={24} sm={12} lg={8} key={p.name}>
                <Card
                  size="small"
                  title={
                    <Flex align="center" gap={6}>
                      <Text strong style={{ fontSize: 14 }}>{p.display_name}</Text>
                      <Tag color={p.protocol === 'anthropic' ? 'purple' : 'blue'} style={{ fontSize: 11, lineHeight: '18px' }}>
                        {p.protocol === 'anthropic' ? 'Anthropic' : 'OpenAI'}
                      </Tag>
                    </Flex>
                  }
                  extra={
                    <Space size={4}>
                      <Button type="link" size="small" icon={<EditOutlined />} onClick={() => openEdit(p.name)} />
                      <Button type="link" size="small" danger icon={<DeleteOutlined />} onClick={() => handleDelete(p.name)} />
                    </Space>
                  }
                  style={{ borderRadius: 8, height: '100%' }}
                  actions={[
                    <Button type="primary" size="small" icon={<SwapOutlined />}
                      onClick={() => openApply(p)} style={{ width: '90%' }}>
                      应用
                    </Button>,
                  ]}
                >
                  <div style={{ fontSize: 12, lineHeight: '22px' }}>
                    <div><Text type="secondary">Key: </Text>{maskKey(p.api_key)}</div>
                    <div><Text type="secondary">URL: </Text><Text style={{ wordBreak: 'break-all', fontSize: 12 }}>{p.base_url}</Text></div>
                    <div style={{ marginTop: 4 }}>
                      <Text type="secondary">模型: </Text>
                      {p.models.map(m => (
                        <Tag key={m.name} style={{ fontSize: 11, marginBottom: 2 }}>
                          {m.display_name || m.name}
                          {m.supports_1m_context && <Text style={{ fontSize: 10, color: '#fa8c16' }}> 1M</Text>}
                        </Tag>
                      ))}
                    </div>
                  </div>
                </Card>
              </Col>
            ))}
          </Row>
        </Spin>
      </PageCard>

      {/* 新建/编辑弹窗 */}
      {formModal(createVisible || editVisible,
        () => { setCreateVisible(false); setEditVisible(false); },
        editingName ? '编辑 API Key' : '新增 API Key'
      )}

      {/* 应用弹窗 */}
      <Modal title={`应用 ${applyProvider?.display_name || ''}`}
        open={applyVisible}
        onCancel={() => { setApplyVisible(false); setApplyResult(null); }}
        footer={applyResult ? (
          <Button type="primary" onClick={() => { setApplyVisible(false); setApplyResult(null); }}>确定</Button>
        ) : (
          <Space>
            <Button onClick={() => { setApplyVisible(false); setApplyResult(null); }}>取消</Button>
            <Button type="primary" loading={applying} disabled={selectedExecutors.length === 0}
              onClick={handleApply}>应用</Button>
          </Space>
        )}
        width={isMobile ? '100%' : 560}
        style={isMobile ? { top: 0, maxWidth: '100%', margin: 0 } : undefined}
      >
        {!applyResult ? (
          <>
            <Paragraph type="secondary">
              选择要应用此 API Key 的执行器，系统将按各执行器的格式写入配置文件。
            </Paragraph>
            <div style={{ maxHeight: 320, overflowY: 'auto' }}>
              <Checkbox.Group value={selectedExecutors} onChange={setSelectedExecutors as any}
                style={{ width: '100%' }}>
                <Row>
                  {ALL_EXECUTORS.map(exe => (
                    <Col span={12} key={exe.value} style={{ marginBottom: 8 }}>
                      <Checkbox value={exe.value}>{exe.label}</Checkbox>
                    </Col>
                  ))}
                </Row>
              </Checkbox.Group>
            </div>
          </>
        ) : (
          <div>
            <Alert type={applyResult.errors.length > 0 ? 'warning' : 'success'}
              message={`已应用到 ${applyResult.applied.length} 个执行器`}
              description={applyResult.errors.length > 0 ? `${applyResult.errors.length} 个失败` : undefined}
              showIcon style={{ marginBottom: 16 }}
            />
            {applyResult.applied.length > 0 && (
              <div style={{ marginBottom: 8 }}>
                <Text strong>成功：</Text>
                <div style={{ marginTop: 4 }}>
                  {applyResult.applied.map(n => <Tag key={n} color="success">{n}</Tag>)}
                </div>
              </div>
            )}
            {applyResult.errors.length > 0 && (
              <div>
                <Text strong type="danger">失败：</Text>
                <ul style={{ marginTop: 4, paddingLeft: 20 }}>
                  {applyResult.errors.map((e, i) => <li key={i}><Text type="danger">{e}</Text></li>)}
                </ul>
              </div>
            )}
          </div>
        )}
      </Modal>
    </>
  );
}

/** API Key 中间 4 位替换为 * */
function maskKey(key: string): string {
  if (key.length <= 8) return '****';
  return key.slice(0, 4) + '****' + key.slice(-4);
}
