import { useState, useEffect } from 'react';
import { Tabs, Spin, Button, Card, List, Empty, Modal, Input, AutoComplete, Space, Popconfirm, Switch, Tooltip, Typography, message } from 'antd';
import { PlusOutlined, EditOutlined, DeleteOutlined, CopyOutlined, ReloadOutlined, ClockCircleOutlined, QuestionCircleOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CronPresetSelect } from '@/components/CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '@/utils/cron';
import * as db from '@/utils/database';
import type { TodoTemplate, CustomTemplateStatus } from '@/types';


export function TemplatesPanel() {
  const [templates, setTemplates] = useState<TodoTemplate[]>([]);
  const [templatesLoading, setTemplatesLoading] = useState(false);
  const [templateEditing, setTemplateEditing] = useState<TodoTemplate | null>(null);
  const [templateFormOpen, setTemplateFormOpen] = useState(false);
  const [templateFormTitle, setTemplateFormTitle] = useState('');
  const [templateFormPrompt, setTemplateFormPrompt] = useState('');
  const [templateFormCategory, setTemplateFormCategory] = useState('');
  const [templateFormSaving, setTemplateFormSaving] = useState(false);

  const [customTemplateStatus, setCustomTemplateStatus] = useState<CustomTemplateStatus | null>(null);
  const [customTemplateLoading, setCustomTemplateLoading] = useState(false);
  const [customTemplateSubscribing, setCustomTemplateSubscribing] = useState(false);
  const [customTemplateUrl, setCustomTemplateUrl] = useState('');
  const [customTemplateAutoSyncEnabled, setCustomTemplateAutoSyncEnabled] = useState(false);
  const [customTemplateAutoSyncCron, setCustomTemplateAutoSyncCron] = useState('0 0 4 * * *');

  // Load todo templates
  useEffect(() => {
    setTemplatesLoading(true);
    db.getTodoTemplates()
      .then((list) => {
        setTemplates(list);
      })
      .catch((err) => {
        message.error('加载模板失败: ' + (err?.message || String(err)));
      })
      .finally(() => setTemplatesLoading(false));
  }, []);

  const loadCustomTemplateStatus = () => {
    setCustomTemplateLoading(true);
    db.getCustomTemplateStatus()
      .then((status) => {
        setCustomTemplateStatus(status);
        setCustomTemplateAutoSyncEnabled(status.auto_sync_enabled);
        setCustomTemplateAutoSyncCron(status.auto_sync_cron);
      })
      .catch((err) => {
        console.error('加载自定义模板状态失败:', err);
      })
      .finally(() => setCustomTemplateLoading(false));
  };

  useEffect(() => {
    loadCustomTemplateStatus();
  }, []);

  const openTemplateForm = (template?: TodoTemplate) => {
    if (template) {
      setTemplateEditing(template);
      setTemplateFormTitle(template.title);
      setTemplateFormPrompt(template.prompt || '');
      setTemplateFormCategory(template.category);
    } else {
      setTemplateEditing(null);
      setTemplateFormTitle('');
      setTemplateFormPrompt('');
      setTemplateFormCategory('');
    }
    setTemplateFormOpen(true);
  };

  const closeTemplateForm = () => {
    setTemplateFormOpen(false);
    setTemplateEditing(null);
  };

  const handleSaveTemplate = async () => {
    const title = templateFormTitle.trim();
    const prompt = templateFormPrompt.trim();
    const category = templateFormCategory.trim();
    if (!title) {
      message.error('请输入模板标题');
      return;
    }
    if (!category) {
      message.error('请输入模板分类');
      return;
    }
    setTemplateFormSaving(true);
    try {
      if (templateEditing) {
        await db.updateTodoTemplate(templateEditing.id, title, prompt || null, category);
        setTemplates(prev => prev.map(t => t.id === templateEditing.id ? { ...t, title, prompt: prompt || null, category } : t));
        message.success('模板已更新');
      } else {
        const newTemplate = await db.createTodoTemplate(title, prompt || null, category);
        setTemplates(prev => [...prev, newTemplate]);
        message.success('模板已创建');
      }
      closeTemplateForm();
    } catch (err: any) {
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setTemplateFormSaving(false);
    }
  };

  const handleDeleteTemplate = async (templateId: number) => {
    try {
      await db.deleteTodoTemplate(templateId);
      setTemplates(prev => prev.filter(t => t.id !== templateId));
      message.success('模板已删除');
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

  const handleCopyTemplate = async (templateId: number) => {
    try {
      const newTemplate = await db.copyTodoTemplate(templateId);
      setTemplates(prev => [...prev, newTemplate]);
      message.success('模板已复制');
    } catch (err: any) {
      message.error('复制失败: ' + (err?.message || String(err)));
    }
  };

  const handleSubscribeCustomTemplate = async () => {
    if (!customTemplateUrl.trim()) {
      message.error('请输入模板地址');
      return;
    }
    setCustomTemplateSubscribing(true);
    try {
      const status = await db.subscribeCustomTemplate(customTemplateUrl.trim());
      setCustomTemplateStatus(status);
      const list = await db.getTodoTemplates();
      setTemplates(list);
      message.success('订阅成功');
    } catch (err: any) {
      message.error('订阅失败: ' + (err?.message || String(err)));
    } finally {
      setCustomTemplateSubscribing(false);
    }
  };

  const handleUnsubscribeCustomTemplate = async () => {
    try {
      await db.unsubscribeCustomTemplate();
      setCustomTemplateStatus(null);
      setCustomTemplateUrl('');
      const list = await db.getTodoTemplates();
      setTemplates(list);
      message.success('已取消订阅');
    } catch (err: any) {
      message.error('取消订阅失败: ' + (err?.message || String(err)));
    }
  };

  const handleSyncCustomTemplate = async () => {
    setCustomTemplateLoading(true);
    try {
      const status = await db.syncCustomTemplate();
      setCustomTemplateStatus(status);
      const list = await db.getTodoTemplates();
      setTemplates(list);
      message.success('同步成功');
    } catch (err: any) {
      message.error('同步失败: ' + (err?.message || String(err)));
    } finally {
      setCustomTemplateLoading(false);
    }
  };

  const handleUpdateCustomTemplateAutoSync = async () => {
    try {
      await db.updateCustomTemplateAutoSync(customTemplateAutoSyncEnabled, customTemplateAutoSyncCron);
      message.success('自动同步配置已更新');
    } catch (err: any) {
      message.error('更新失败: ' + (err?.message || String(err)));
    }
  };

  return (
    <div style={{ maxWidth: 700 }}>
      <Spin spinning={templatesLoading}>
        <Tabs
          defaultActiveKey="user"
          items={[
            {
              key: 'user',
              label: '我的模板',
              children: (
                <div>
                  <div style={{ marginBottom: 16 }}>
                    <Button type="primary" icon={<PlusOutlined />} onClick={() => openTemplateForm()}>
                      新建模板
                    </Button>
                  </div>
                  {templates.filter(t => !t.is_system && !t.source_url).length === 0 ? (
                    <Empty description="暂无用户模板" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                  ) : (
                    Array.from(new Set(templates.filter(t => !t.is_system && !t.source_url).map(t => t.category))).sort().map(category => (
                      <Card key={category} title={category || '未分类'} size="small" style={{ marginBottom: 12 }}>
                        <List
                          dataSource={templates.filter(t => !t.is_system && !t.source_url && t.category === category)}
                          renderItem={(template) => (
                            <List.Item
                              style={{ padding: '8px 0' }}
                              actions={[
                                <Button key="edit" type="text" icon={<EditOutlined />} size="small" onClick={() => openTemplateForm(template)} />,
                                <Popconfirm key="delete" title="删除模板" description={`确定要删除模板 "${template.title}" 吗？`} onConfirm={() => handleDeleteTemplate(template.id)}>
                                  <Button type="text" icon={<DeleteOutlined />} size="small" />
                                </Popconfirm>,
                              ]}
                            >
                              <List.Item.Meta
                                title={template.title}
                                description={template.prompt || '(无内容)'}
                              />
                            </List.Item>
                          )}
                        />
                      </Card>
                    ))
                  )}
                </div>
              ),
            },
            {
              key: 'system',
              label: '内置模板',
              children: (
                <div>
                  {templates.filter(t => t.is_system).length === 0 ? (
                    <Empty description="暂无内置模板" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                  ) : (
                    Array.from(new Set(templates.filter(t => t.is_system).map(t => t.category))).sort().map(category => (
                      <Card key={category} title={category || '未分类'} size="small" style={{ marginBottom: 12 }}>
                        <List
                          dataSource={templates.filter(t => t.is_system && t.category === category)}
                          renderItem={(template) => (
                            <List.Item
                              style={{ padding: '8px 0' }}
                              actions={[
                                <Button key="copy" type="text" icon={<CopyOutlined />} size="small" onClick={() => handleCopyTemplate(template.id)}>
                                  复制
                                </Button>,
                              ]}
                            >
                              <List.Item.Meta
                                title={template.title}
                                description={template.prompt || '(无内容)'}
                              />
                            </List.Item>
                          )}
                        />
                      </Card>
                    ))
                  )}
                </div>
              ),
            },
            {
              key: 'custom',
              label: '在线模板',
              children: (
                <div>
                  <Spin spinning={customTemplateLoading}>
                    {customTemplateStatus?.subscribed ? (
                      <div>
                        <Card size="small" style={{ marginBottom: 12 }}>
                          <Space direction="vertical" style={{ width: '100%' }}>
                            <div>
                              <Typography.Text type="secondary">订阅地址：</Typography.Text>
                              <Typography.Text copyable>{customTemplateStatus.source_url}</Typography.Text>
                            </div>
                            {customTemplateStatus.last_sync_at && (
                              <div>
                                <Typography.Text type="secondary">最后同步：</Typography.Text>
                                <Typography.Text>{new Date(customTemplateStatus.last_sync_at).toLocaleString()}</Typography.Text>
                              </div>
                            )}
                            <Space>
                              <Button icon={<ReloadOutlined />} onClick={handleSyncCustomTemplate}>
                                立即同步
                              </Button>
                              <Popconfirm
                                title="取消订阅"
                                description="确定要取消订阅吗？订阅的模板将被删除。"
                                onConfirm={handleUnsubscribeCustomTemplate}
                              >
                                <Button danger>取消订阅</Button>
                              </Popconfirm>
                            </Space>
                          </Space>
                        </Card>

                        <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12, marginTop: 4 }}>
                          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                            <span style={{ fontWeight: 600 }}><ClockCircleOutlined style={{ marginRight: 6 }} />自动同步</span>
                            <Switch checked={customTemplateAutoSyncEnabled} onChange={setCustomTemplateAutoSyncEnabled} />
                          </div>
                          {customTemplateAutoSyncEnabled && (
                            <CronPresetSelect
                              value={customTemplateAutoSyncCron}
                              onChange={(val) => setCustomTemplateAutoSyncCron(val)}
                            />
                          )}
                          {customTemplateAutoSyncEnabled && (
                            <Cron
                              value={cronTo5(customTemplateAutoSyncCron)}
                              setValue={(val: string) => setCustomTemplateAutoSyncCron(cronTo6(val))}
                              locale={CRON_ZH_LOCALE}
                              defaultPeriod="day"
                              humanizeLabels
                              allowClear={false}
                            />
                          )}
                          {customTemplateAutoSyncEnabled && (
                            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
                              <Button size="small" type="primary" onClick={handleUpdateCustomTemplateAutoSync}>
                                保存
                              </Button>
                            </div>
                          )}
                        </div>

                        <div style={{ marginBottom: 8 }}>
                          <Typography.Text strong>模板列表</Typography.Text>
                        </div>
                        {customTemplateStatus.templates.length === 0 ? (
                          <Empty description="暂无在线模板" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                        ) : (
                          Array.from(new Set(customTemplateStatus.templates.map(t => t.category))).sort().map(category => (
                            <Card key={category} title={category || '未分类'} size="small" style={{ marginBottom: 12 }}>
                              <List
                                dataSource={customTemplateStatus.templates.filter(t => t.category === category)}
                                renderItem={(template) => (
                                  <List.Item
                                    style={{ padding: '8px 0' }}
                                    actions={[
                                      <Button key="copy" type="text" icon={<CopyOutlined />} size="small" onClick={() => handleCopyTemplate(template.id)}>
                                        复制
                                      </Button>,
                                    ]}
                                  >
                                    <List.Item.Meta
                                      title={template.title}
                                      description={template.prompt || '(无内容)'}
                                    />
                                  </List.Item>
                                )}
                              />
                            </Card>
                          ))
                        )}
                      </div>
                    ) : (
                      <Card size="small">
                        <Space direction="vertical" style={{ width: '100%' }}>
                          <Space>
                            <Typography.Text>订阅一个在线模板地址</Typography.Text>
                            <Tooltip title={<span>填写在线 YAML 地址，格式参考 GitHub <a href="https://raw.githubusercontent.com/weibaohui/nothing-todo/refs/heads/main/templates.example.yaml" target="_blank">示例</a> 或 GitCode <a href="https://raw.gitcode.com/weibaohui/nothing-todo/raw/main/templates.example.yaml" target="_blank">示例</a></span>}>
                              <span style={{ cursor: 'help' }}><QuestionCircleOutlined /></span>
                            </Tooltip>
                          </Space>
                          <Input
                            placeholder="输入模板地址"
                            value={customTemplateUrl}
                            onChange={(e) => setCustomTemplateUrl(e.target.value)}
                            onPressEnter={handleSubscribeCustomTemplate}
                          />
                          <Button
                            type="primary"
                            loading={customTemplateSubscribing}
                            onClick={handleSubscribeCustomTemplate}
                          >
                            订阅
                          </Button>
                        </Space>
                      </Card>
                    )}
                  </Spin>
                </div>
              ),
            },
          ]}
        />
      </Spin>
      <Modal
        title={templateEditing ? '编辑模板' : '新建模板'}
        open={templateFormOpen}
        onOk={handleSaveTemplate}
        onCancel={closeTemplateForm}
        confirmLoading={templateFormSaving}
        width={500}
      >
        <Space direction="vertical" style={{ width: '100%' }}>
          <div>
            <div style={{ marginBottom: 4, fontWeight: 500 }}>标题</div>
            <Input
              value={templateFormTitle}
              onChange={e => setTemplateFormTitle(e.target.value)}
              placeholder="输入模板标题"
            />
          </div>
          <div>
            <div style={{ marginBottom: 4, fontWeight: 500 }}>分类</div>
            <AutoComplete
              placeholder="输入或选择分类"
              value={templateFormCategory}
              onChange={(value) => setTemplateFormCategory(value)}
              options={Array.from(new Set(templates.map(t => t.category))).filter(c => c).map(c => ({ label: c, value: c }))}
              style={{ width: '100%' }}
              filterOption={(input, option) =>
                (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
              }
            />
          </div>
          <div>
            <div style={{ marginBottom: 4, fontWeight: 500 }}>Prompt 内容</div>
            <Input.TextArea
              value={templateFormPrompt}
              onChange={e => setTemplateFormPrompt(e.target.value)}
              placeholder="输入模板的 prompt 内容（可选）"
              rows={6}
            />
          </div>
        </Space>
      </Modal>
    </div>
  );
}
