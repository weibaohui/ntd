//! API Key 管理面板。
//!
//! 每个 API Key 一张卡片，点击「应用」选择执行器 → 预览内容 → 确认写入。

import { useState, useEffect, useCallback } from 'react';
import { Button, Card, Checkbox, Empty, Form, Input, message, Modal, Space, Spin, Steps, Tag, Typography, Flex, Alert, Row, Col, Tabs } from 'antd';
import { PlusOutlined, SwapOutlined, DeleteOutlined, EditOutlined, DatabaseOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { useIsMobile } from '@/hooks/useIsMobile';

const { Text, Paragraph } = Typography;

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
  // 应用两步弹窗状态
  const APPLY_STEP_SELECT = 1;
  const APPLY_STEP_PREVIEW = 2;
  const [applyVisible, setApplyVisible] = useState(false);
  const [applyStep, setApplyStep] = useState(APPLY_STEP_SELECT);
  const [applyProvider, setApplyProvider] = useState<ProviderDetail | null>(null);
  const [selectedExecutors, setSelectedExecutors] = useState<string[]>([]);
  const [applying, setApplying] = useState(false);
  const [applyResult, setApplyResult] = useState<ApplyResult | null>(null);
  const [previewEntries, setPreviewEntries] = useState<{ executor: string; path: string; content: string }[]>([]);
  const [previewLoading, setPreviewLoading] = useState(false);
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
    setPreviewEntries([]);
    setApplyStep(APPLY_STEP_SELECT);
    setApplyVisible(true);
  }, []);

  // 下一步：获取预览
  const goToPreview = useCallback(async () => {
    if (!applyProvider || selectedExecutors.length === 0) return;
    setPreviewLoading(true);
    try {
      const r = await fetch(`${PROVIDERS_API}/${encodeURIComponent(applyProvider.name)}/preview`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ executors: selectedExecutors }),
      });
      const j = await r.json();
      setPreviewEntries(j.data || []);
      setApplyStep(APPLY_STEP_PREVIEW);
    } catch (err: any) {
      message.error('获取预览失败: ' + (err?.message || String(err)));
    } finally {
      setPreviewLoading(false);
    }
  }, [applyProvider, selectedExecutors]);

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

      {/* 应用弹窗 - 两步流程 */}
      <Modal title={`应用 ${applyProvider?.display_name || ''}`}
        open={applyVisible}
        onCancel={() => { setApplyVisible(false); setApplyResult(null); }}
        footer={
          applyResult ? (
            // 步骤3：结果
            <Button type="primary" onClick={() => { setApplyVisible(false); setApplyResult(null); }}>确定</Button>
          ) : applyStep === APPLY_STEP_SELECT ? (
            // 步骤1：选择执行器
            <Space>
              <Button onClick={() => { setApplyVisible(false); setApplyResult(null); }}>取消</Button>
              <Button type="primary" loading={previewLoading} disabled={selectedExecutors.length === 0}
                onClick={goToPreview}>下一步</Button>
            </Space>
          ) : (
            // 步骤2：预览
            <Space>
              <Button onClick={() => { setApplyStep(APPLY_STEP_SELECT); setApplyResult(null); }}>返回</Button>
              <Button type="primary" loading={applying} onClick={handleApply}>确认写入</Button>
            </Space>
          )
        }
        width={isMobile ? '100%' : 640}
        style={isMobile ? { top: 0, maxWidth: '100%', margin: 0 } : undefined}
      >
        {/* 步骤条 */}
        <Steps
          current={applyStep === APPLY_STEP_SELECT ? 0 : applyResult ? 2 : 1}
          size="small"
          items={[
            { title: '选择执行器' },
            { title: '预览配置' },
            { title: '完成' },
          ]}
          style={{ marginBottom: 20 }}
        />

        {!applyResult ? (
          applyStep === APPLY_STEP_SELECT ? (
            <>
              <Paragraph type="secondary">
                选择要应用此 API Key 的执行器。支持的执行器：
              </Paragraph>
              <div style={{ maxHeight: 320, overflowY: 'auto' }}>
                <Checkbox.Group value={selectedExecutors} onChange={setSelectedExecutors as any}
                  style={{ width: '100%' }}>
                  <Row>
                    {/** 只显示有生成器的 4 个执行器 */}
                    {[
                      { value: 'claudecode', label: 'Claude Code', path: '~/.claude/settings.json' },
                      { value: 'pi', label: 'PI', path: '~/.pi/config.yaml' },
                      { value: 'atomcode', label: 'AtomCode', path: '~/.atomcode/config.toml' },
                      { value: 'kilo', label: 'Kilo', path: '~/.kilo/config.json' },
                    ].map(exe => (
                      <Col span={24} key={exe.value} style={{ marginBottom: 4 }}>
                        <Checkbox value={exe.value}>
                          <Text>{exe.label}</Text>
                          <Text type="secondary" style={{ fontSize: 12, marginLeft: 8 }}>{exe.path}</Text>
                        </Checkbox>
                      </Col>
                    ))}
                  </Row>
                </Checkbox.Group>
              </div>
            </>
          ) : (
            // 步骤2：预览 — 每个执行器一个 Tab
            <>
              <Paragraph type="secondary">
                确认以下文件内容，然后点击「确认写入」。如有问题可返回重新选择。
              </Paragraph>
              <Tabs
                size="small"
                items={previewEntries.map(entry => ({
                  key: entry.executor,
                  label: entry.executor,
                  children: (
                    <div>
                      <Text type="secondary" style={{ fontSize: 12, display: 'block', marginBottom: 8 }}>
                        写入路径：{entry.path || '（未知）'}
                      </Text>
                      <pre style={{
                        background: '#f5f5f5',
                        padding: 12,
                        borderRadius: 6,
                        fontSize: 12,
                        lineHeight: 1.5,
                        overflow: 'auto',
                        maxHeight: 400,
                        whiteSpace: 'pre-wrap',
                        wordBreak: 'break-all',
                        margin: 0,
                      }}>{entry.content}</pre>
                    </div>
                  ),
                }))}
              />
            </>
          )
        ) : (
          // 步骤3：结果
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
                  {applyResult.applied.map((n: string) => <Tag key={n} color="success">{n}</Tag>)}
                </div>
              </div>
            )}
            {applyResult.errors.length > 0 && (
              <div>
                <Text strong type="danger">失败：</Text>
                <ul style={{ marginTop: 4, paddingLeft: 20 }}>
                  {applyResult.errors.map((e: string, i: number) => <li key={i}><Text type="danger">{e}</Text></li>)}
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
