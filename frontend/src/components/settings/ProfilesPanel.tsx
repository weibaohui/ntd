//! API Key 管理面板。
//!
//! 每个 API Key 一张卡片，点击「应用」选择执行器 → 预览内容 → 确认写入。

import { useState, useEffect, useCallback } from 'react';
import { Button, Card, Checkbox, Empty, Form, Input, message, Modal, Popconfirm, Select, Space, Spin, Steps, Tag, Typography, Flex, Alert, Row, Col, Tabs } from 'antd';
import { PlusOutlined, SwapOutlined, DeleteOutlined, EditOutlined, DatabaseOutlined, DownloadOutlined, UploadOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { useIsMobile } from '@/hooks/useIsMobile';
import { useTheme } from '@/hooks/useTheme';

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
  // 每个执行器选中的模型: { executor_name: model_name }
  const [executorModels, setExecutorModels] = useState<Record<string, string>>({});
  const [applying, setApplying] = useState(false);
  const [applyResult, setApplyResult] = useState<ApplyResult | null>(null);
  const [previewEntries, setPreviewEntries] = useState<{ executor: string; model: string; path: string; content: string }[]>([]);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [executorDefs, setExecutorDefs] = useState<{ name: string; display_name: string; config_path: string; has_generator: boolean }[]>([]);
  const [modelList, setModelList] = useState<ProviderModel[]>([]);
  // 导入弹窗
  const [importModalVisible, setImportModalVisible] = useState(false);
  const [importText, setImportText] = useState('');
  const [importFileName, setImportFileName] = useState('');
  const [importStrategy, setImportStrategy] = useState<'merge' | 'replace'>('merge');
  const [importing, setImporting] = useState(false);
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';
  const [form] = Form.useForm();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [provRes, execRes] = await Promise.all([
        fetch(PROVIDERS_API),
        fetch(`${PROVIDERS_API}/supported-executors`),
      ]);
      const provJson = await provRes.json();
      const execJson = await execRes.json();
      const list: any[] = provJson.data || [];
      setExecutorDefs(execJson.data || []);
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

  // 导出：调后端 export 接口，触发文件下载
  const handleExport = useCallback(async () => {
    try {
      const r = await fetch(`${PROVIDERS_API}/export`, { method: 'GET' });
      if (!r.ok) {
        message.error('导出失败: HTTP ' + r.status);
        return;
      }
      const yaml = await r.text();
      const filename = `ntd-providers-${new Date().toISOString().slice(0, 10)}.yaml`;
      const blob = new Blob([yaml], { type: 'text/yaml;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success(`已导出 ${filename}`);
    } catch (err: any) {
      message.error('导出失败: ' + (err?.message || String(err)));
    }
  }, []);

  // 导入：用户选文件或粘贴文本，调后端 import 接口
  const handleImport = useCallback(async () => {
    if (!importText.trim()) {
      message.warning('请选择 YAML 文件或粘贴内容');
      return;
    }
    setImporting(true);
    try {
      const r = await fetch(`${PROVIDERS_API}/import`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ yaml: importText, strategy: importStrategy }),
      });
      const j = await r.json();
      if (j.code !== 0) {
        message.error('导入失败: ' + (j.message || '未知错误'));
        return;
      }
      const { imported = [], errors = [] } = j.data || {};
      if (errors.length > 0) {
        message.warning(`导入完成，${imported.length} 个成功，${errors.length} 个错误`);
      } else {
        message.success(`成功导入 ${imported.length} 个 Provider`);
      }
      setImportModalVisible(false);
      setImportText('');
      setImportFileName('');
      load();
    } catch (err: any) {
      message.error('导入失败: ' + (err?.message || String(err)));
    } finally {
      setImporting(false);
    }
  }, [importText, importStrategy, load]);

  // 读取上传文件内容
  const handleImportFile = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setImportFileName(file.name);
    const reader = new FileReader();
    reader.onload = () => setImportText(String(reader.result || ''));
    reader.readAsText(file);
  }, []);

  // 打开应用弹窗
  const openApply = useCallback((p: ProviderDetail) => {
    setApplyProvider(p);
    setSelectedExecutors([]);
    setExecutorModels({});
    setApplyResult(null);
    setPreviewEntries([]);
    setApplyStep(APPLY_STEP_SELECT);
    setApplyVisible(true);
  }, []);

  // 切换执行器选择时，同步初始化模型选择（已通过 onClick 内联处理）

  // 下一步：获取预览
  const goToPreview = useCallback(async () => {
    if (!applyProvider || selectedExecutors.length === 0) return;
    setPreviewLoading(true);
    try {
      const r = await fetch(`${PROVIDERS_API}/${encodeURIComponent(applyProvider.name)}/preview`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ executor_models: executorModels }),
      });
      const j = await r.json();
      setPreviewEntries(j.data || []);
      setApplyStep(APPLY_STEP_PREVIEW);
    } catch (err: any) {
      message.error('获取预览失败: ' + (err?.message || String(err)));
    } finally {
      setPreviewLoading(false);
    }
  }, [applyProvider, executorModels, selectedExecutors]);

  // 执行应用
  const handleApply = useCallback(async () => {
    if (!applyProvider || Object.keys(executorModels).length === 0) return;
    setApplying(true);
    try {
      const r = await fetch(`${PROVIDERS_API}/${encodeURIComponent(applyProvider.name)}/apply`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ executor_models: executorModels }),
      });
      const j = await r.json();
      setApplyResult(j.data);
    } catch (err: any) {
      message.error('应用失败: ' + (err?.message || String(err)));
    } finally {
      setApplying(false);
    }
  }, [applyProvider, executorModels]);

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
        <Form.Item name="protocol" label="协议格式" initialValue="openai" style={{ maxWidth: 200 }} rules={[{ required: true, message: '必选' }]}>
          <Select
            options={[
              { value: 'openai', label: 'OpenAI 兼容' },
              { value: 'anthropic', label: 'Anthropic 原生' },
            ]}
          />
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
            <Flex gap={8} wrap="wrap">
              <Button type="primary" size="small" icon={<PlusOutlined />} onClick={openCreate}>新增</Button>
              <Button size="small" icon={<DownloadOutlined />} onClick={handleExport}>导出</Button>
              <Button size="small" icon={<UploadOutlined />} onClick={() => setImportModalVisible(true)}>导入</Button>
              <Button size="small" onClick={load} loading={loading}>刷新</Button>
            </Flex>
          ) : (
            <Space>
              <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>新增 API Key</Button>
              <Button icon={<DownloadOutlined />} onClick={handleExport}>导出</Button>
              <Button icon={<UploadOutlined />} onClick={() => setImportModalVisible(true)}>导入</Button>
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
                      <Popconfirm
                        title="确认删除？"
                        description={`将删除 API Key "${p.display_name}"，此操作不可恢复。`}
                        okText="删除"
                        cancelText="取消"
                        okButtonProps={{ danger: true }}
                        onConfirm={() => handleDelete(p.name)}
                      >
                        <Button type="link" size="small" danger icon={<DeleteOutlined />} />
                      </Popconfirm>
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
              <div style={{ maxHeight: 400, overflowY: 'auto' }}>
                {executorDefs.filter(d => d.has_generator).map(def => {
                  const checked = selectedExecutors.includes(def.name);
                  const models = applyProvider?.models || [];
                  return (
                    <div key={def.name}
                      onClick={() => {
                        if (!checked) {
                          // 选中时：添加到 selectedExecutors + 设置默认模型
                          setSelectedExecutors(prev => [...prev, def.name]);
                          if (!executorModels[def.name] && models.length > 0) {
                            setExecutorModels(prev => ({ ...prev, [def.name]: models[0].name }));
                          }
                        } else {
                          setSelectedExecutors(prev => prev.filter(e => e !== def.name));
                        }
                      }}
                      style={{
                        padding: '10px 12px',
                        marginBottom: 6,
                        borderRadius: 6,
                        cursor: 'pointer',
                        border: checked ? '1px solid #1677ff' : '1px solid #f0f0f0',
                        background: checked ? 'rgba(22,119,255,0.04)' : 'transparent',
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                        <input type="checkbox" checked={checked} readOnly
                          style={{ width: 16, height: 16, cursor: 'pointer', accentColor: '#1677ff' }} />
                        <Text strong style={{ fontSize: 14 }}>{def.display_name}</Text>
                      </div>

                      {checked && (
                        <div style={{ marginTop: 8, marginLeft: 24 }}>
                          {models.length > 0 && (
                            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                              <Text type="secondary" style={{ fontSize: 12, whiteSpace: 'nowrap' }}>模型：</Text>
                              <select
                                value={executorModels[def.name] || models[0]?.name || ''}
                                onChange={e => setExecutorModels(prev => ({ ...prev, [def.name]: e.target.value }))}
                                onClick={e => e.stopPropagation()}
                                style={{
                                  padding: '3px 8px',
                                  borderRadius: 4,
                                  border: '1px solid #d9d9d9',
                                  fontSize: 13,
                                  background: 'inherit',
                                  color: 'inherit',
                                  maxWidth: 200,
                                }}
                              >
                                {models.map(m => (
                                  <option key={m.name} value={m.name}>
                                    {m.display_name || m.name}{m.supports_1m_context ? ' [1M]' : ''}
                                  </option>
                                ))}
                              </select>
                              <Text type="secondary" style={{ fontSize: 11 }}>（默认）</Text>
                            </div>
                          )}
                          <Text type="secondary" style={{ fontSize: 11, display: 'block' }}>
                            写入路径：{def.config_path}
                          </Text>
                        </div>
                      )}
                    </div>
                  );
                })}
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
                  label: `${entry.executor} — ${entry.model}`,
                  children: (
                    <div>
                      <Text type="secondary" style={{ fontSize: 12, display: 'block', marginBottom: 8 }}>
                        写入路径：{entry.path || '（未知）'}
                      </Text>
                      <pre style={{
                        background: isDark ? '#1a1a2e' : '#f5f5f5',
                        color: isDark ? '#e0e0e0' : '#333',
                        padding: 12,
                        borderRadius: 6,
                        fontSize: 12,
                        lineHeight: 1.5,
                        overflow: 'auto',
                        maxHeight: 400,
                        whiteSpace: 'pre-wrap',
                        wordBreak: 'break-all',
                        margin: 0,
                        border: isDark ? '1px solid #2a2a4a' : 'none',
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

      {/* 导入弹窗 */}
      <Modal
        title="导入 API Key"
        open={importModalVisible}
        onCancel={() => { setImportModalVisible(false); setImportText(''); setImportFileName(''); }}
        footer={
          <Space>
            <Button onClick={() => { setImportModalVisible(false); setImportText(''); setImportFileName(''); }}>取消</Button>
            <Button type="primary" loading={importing} onClick={handleImport}>
              确认导入
            </Button>
          </Space>
        }
        width={isMobile ? '100%' : 640}
        style={isMobile ? { top: 0, maxWidth: '100%', margin: 0 } : undefined}
      >
        <Paragraph type="secondary">
          选 YAML 文件上传，或直接粘贴内容。导入仅修改 <code>providers</code> 段，不影响 <code>profiles</code> 和 <code>current_profile</code>。
        </Paragraph>
        <div style={{ marginBottom: 12 }}>
          <label
            style={{
              display: 'inline-block',
              padding: '6px 14px',
              border: '1px dashed #d9d9d9',
              borderRadius: 6,
              cursor: 'pointer',
            }}
          >
            选择文件: {importFileName || '未选择'}
            <input type="file" accept=".yaml,.yml" style={{ display: 'none' }} onChange={handleImportFile} />
          </label>
        </div>
        <div style={{ marginBottom: 12 }}>
          <Text type="secondary" style={{ fontSize: 12 }}>
            或直接粘贴 YAML：
          </Text>
        </div>
        <Input.TextArea
          rows={10}
          value={importText}
          onChange={e => setImportText(e.target.value)}
          placeholder="providers:\n  deepseek-anthropic:\n    api_key: sk-xxx\n    ..."
          style={{ fontFamily: 'monospace', fontSize: 12 }}
        />
        <div style={{ marginTop: 12 }}>
          <Text type="secondary" style={{ marginRight: 8 }}>冲突策略：</Text>
          <label style={{ marginRight: 12 }}>
            <input type="radio" checked={importStrategy === 'merge'}
              onChange={() => setImportStrategy('merge')} />
            <span style={{ marginLeft: 4 }}>合并（已存在则覆盖）</span>
          </label>
          <label>
            <input type="radio" checked={importStrategy === 'replace'}
              onChange={() => setImportStrategy('replace')} />
            <span style={{ marginLeft: 4 }}>替换（先清空所有）</span>
          </label>
        </div>
      </Modal>
    </>
  );
}

/** API Key 中间 4 位替换为 * */
function maskKey(key: string): string {
  if (key.length <= 8) return '****';
  return key.slice(0, 4) + '****' + key.slice(-4);
}
