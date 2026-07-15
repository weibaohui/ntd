// 专家模板 Tab
// 提供专家模板的列表、删除、编辑、同步功能
// 数据从内存索引加载，编辑直接修改文件系统

import { useState, useEffect, useCallback } from 'react';
import {
  App,
  Button,
  Empty,
  Modal,
  Popconfirm,
  Space,
  Spin,
  Table,
  Tag,
  Form,
  Input,
  message as antMessage,
  Tabs as AntTabs,
} from 'antd';
import {
  ReloadOutlined,
  EditOutlined,
  DeleteOutlined,
  EyeOutlined,
} from '@ant-design/icons';
import * as db from '@/utils/database';
import type { ExpertMetadata } from '@/types/expert';
import {
  getExpertDisplayName,
  getExpertDescription,
  getExpertProfession,
} from '@/types/expert';

/**
 * 专家模板 Tab
 *
 * 以表格形式展示专家列表，提供：
 * - 刷新列表
 * - 查看详情
 * - 编辑专家（plugin.json + agent.md）
 * - 删除专家
 */
export function ExpertsTemplatesTab() {
  const { message } = App.useApp();
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState<ExpertMetadata | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailExpert, setDetailExpert] = useState<ExpertMetadata | null>(null);
  const [detailAgentMd, setDetailAgentMd] = useState('');
  const [detailSkills, setDetailSkills] = useState<import('@/types/expert').SkillMetadata[]>([]);

  const loadExperts = useCallback(async () => {
    setLoading(true);
    try {
      const list = await db.getAllExperts();
      setExperts(list);
    } catch (e: any) {
      message.error('加载专家列表失败: ' + (e?.message || e));
    } finally {
      setLoading(false);
    }
  }, [message]);

  useEffect(() => {
    loadExperts();
  }, [loadExperts]);

  const handleDelete = async (name: string) => {
    try {
      await db.deleteExpert(name);
      message.success('已删除');
      await loadExperts();
    } catch (e: any) {
      message.error('删除失败: ' + (e?.message || e));
    }
  };

  const handleOpenDetail = async (expert: ExpertMetadata) => {
    setDetailExpert(expert);
    setDetailOpen(true);
    setDetailAgentMd('');
    setDetailSkills([]);
    try {
      const [mdContent, skillList] = await Promise.all([
        db.getExpertAgentMd(expert.name).catch(() => ''),
        db.getExpertSkills(expert.name).catch(() => [] as import('@/types/expert').SkillMetadata[]),
      ]);
      setDetailAgentMd(mdContent);
      setDetailSkills(skillList);
    } catch {
      // 加载失败不阻塞展示
    }
  };

  const handleOpenEdit = (expert: ExpertMetadata) => {
    setEditing(expert);
    setModalOpen(true);
  };

  return (
    <div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Button icon={<ReloadOutlined />} onClick={loadExperts} loading={loading}>
          刷新
        </Button>
      </Space>

      <Spin spinning={loading}>
        {experts.length === 0 ? (
          <Empty description="暂无专家模板" />
        ) : (
          <Table
            rowKey="name"
            dataSource={experts}
            pagination={false}
            scroll={{ x: 'max-content' }}
            columns={[
              {
                title: '名称',
                key: 'name',
                render: (_, record: ExpertMetadata) => (
                  <Space>
                    <span>{getExpertDisplayName(record)}</span>
                    {record.source === 'system' && <Tag color="blue">系统</Tag>}
                    {record.source === 'user' && <Tag color="green">用户</Tag>}
                    {record.expert_type === 'team' && <Tag color="purple">团队</Tag>}
                  </Space>
                ),
              },
              {
                title: '职业',
                key: 'profession',
                width: 140,
                render: (_, record: ExpertMetadata) => getExpertProfession(record) || '-',
              },
              {
                title: '描述',
                key: 'description',
                ellipsis: true,
                render: (_, record: ExpertMetadata) => {
                  const desc = getExpertDescription(record);
                  return desc ? desc.substring(0, 60) + (desc.length > 60 ? '...' : '') : '-';
                },
              },
              {
                title: '版本',
                dataIndex: 'version',
                key: 'version',
                width: 80,
              },
              {
                title: '操作',
                key: 'actions',
                width: 200,
                render: (_, record: ExpertMetadata) => (
                  <Space>
                    <Button
                      type="text"
                      size="small"
                      icon={<EyeOutlined />}
                      onClick={() => handleOpenDetail(record)}
                    />
                    <Button
                      type="text"
                      size="small"
                      icon={<EditOutlined />}
                      onClick={() => handleOpenEdit(record)}
                    />
                    <Popconfirm
                      title="确定删除此专家？"
                      onConfirm={() => handleDelete(record.name)}
                    >
                      <Button type="text" size="small" icon={<DeleteOutlined />} />
                    </Popconfirm>
                  </Space>
                ),
              },
            ]}
          />
        )}
      </Spin>

      {/* 编辑弹窗 */}
      <ExpertEditModal
        open={modalOpen}
        expert={editing}
        onClose={() => setModalOpen(false)}
        onSaved={async () => {
          setModalOpen(false);
          await loadExperts();
        }}
      />

      {/* 详情弹窗 */}
      <ExpertDetailModal
        open={detailOpen}
        expert={detailExpert}
        agentMd={detailAgentMd}
        skills={detailSkills}
        onClose={() => setDetailOpen(false)}
      />
    </div>
  );
}

/**
 * 专家编辑弹窗
 *
 * 提供 plugin.json 和 agent.md 的文本编辑能力。
 * 加载时自动从后端获取当前文件内容。
 */
function ExpertEditModal({
  open,
  expert,
  onClose,
  onSaved,
}: {
  open: boolean;
  expert: ExpertMetadata | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [form] = Form.useForm();
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(false);
  const [activeTab, setActiveTab] = useState('plugin');

  useEffect(() => {
    if (open && expert) {
      setLoading(true);
      setActiveTab('plugin');
      // 并行加载 plugin.json 和 agent.md
      Promise.all([
        fetchPluginJson(expert),
        db.getExpertAgentMd(expert.name).catch(() => ''),
      ])
        .then(([pluginJson, agentMd]) => {
          form.setFieldsValue({
            plugin_json: pluginJson,
            agent_md: agentMd,
          });
        })
        .catch(() => {
          antMessage.error('加载专家文件失败');
        })
        .finally(() => setLoading(false));
    }
  }, [open, expert, form]);

  const fetchPluginJson = async (e: ExpertMetadata): Promise<string> => {
    try {
      return await db.getExpertPluginJson(e.name);
    } catch {
      return '{}';
    }
  };

  const handleSave = async () => {
    if (!expert) return;
    try {
      const values = await form.validateFields();
      setSaving(true);
      await db.updateExpert(expert.name, values.plugin_json, values.agent_md);
      antMessage.success('保存成功');
      onSaved();
    } catch (e: any) {
      if (e?.errorFields) return;
      antMessage.error('保存失败: ' + (e?.message || e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={expert ? `编辑专家：${getExpertDisplayName(expert)}` : '编辑专家'}
      open={open}
      onCancel={onClose}
      onOk={handleSave}
      confirmLoading={saving}
      width={800}
      destroyOnClose
    >
      <Spin spinning={loading}>
        <Form form={form} layout="vertical" style={{ marginTop: 16 }}>
          <AntTabs
            activeKey={activeTab}
            onChange={setActiveTab}
            items={[
              {
                key: 'plugin',
                label: 'plugin.json',
                children: (
                  <Form.Item
                    name="plugin_json"
                    rules={[{ required: true, message: 'plugin.json 不能为空' }]}
                  >
                    <Input.TextArea
                      rows={18}
                      placeholder="专家定义 JSON"
                      style={{ fontFamily: 'monospace', fontSize: 13 }}
                    />
                  </Form.Item>
                ),
              },
              {
                key: 'agent',
                label: 'agent.md',
                children: (
                  <Form.Item name="agent_md">
                    <Input.TextArea
                      rows={18}
                      placeholder="Agent 提示词 Markdown"
                      style={{ fontFamily: 'monospace', fontSize: 13 }}
                    />
                  </Form.Item>
                ),
              },
            ]}
          />
        </Form>
      </Spin>
    </Modal>
  );
}

/**
 * 专家详情弹窗
 *
 * 只读展示专家的基本信息、技能列表和 Agent MD 内容。
 */
function ExpertDetailModal({
  open,
  expert,
  agentMd,
  skills,
  onClose,
}: {
  open: boolean;
  expert: ExpertMetadata | null;
  agentMd: string;
  skills: import('@/types/expert').SkillMetadata[];
  onClose: () => void;
}) {
  return (
    <Modal
      title={expert ? getExpertDisplayName(expert) : '专家详情'}
      open={open}
      onCancel={onClose}
      footer={[
        <Button key="close" onClick={onClose}>
          关闭
        </Button>,
      ]}
      width={700}
    >
      {expert && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <div>
            <div style={{ fontWeight: 'bold', marginBottom: 4 }}>名称</div>
            <div>{getExpertDisplayName(expert)}</div>
          </div>
          {getExpertProfession(expert) && (
            <div>
              <div style={{ fontWeight: 'bold', marginBottom: 4 }}>职业</div>
              <div>{getExpertProfession(expert)}</div>
            </div>
          )}
          {getExpertDescription(expert) && (
            <div>
              <div style={{ fontWeight: 'bold', marginBottom: 4 }}>描述</div>
              <div>{getExpertDescription(expert)}</div>
            </div>
          )}
          <div>
            <div style={{ fontWeight: 'bold', marginBottom: 4 }}>类型</div>
            <Space wrap>
              <Tag color={expert.source === 'system' ? 'blue' : 'green'}>
                {expert.source === 'system' ? '系统内置' : '用户自定义'}
              </Tag>
              <Tag color={expert.expert_type === 'team' ? 'purple' : 'default'}>
                {expert.expert_type === 'team' ? '专家团队' : '单个专家'}
              </Tag>
            </Space>
          </div>
          <div>
            <div style={{ fontWeight: 'bold', marginBottom: 4 }}>版本</div>
            <div>{expert.version}</div>
          </div>
          {skills.length > 0 && (
            <div>
              <div style={{ fontWeight: 'bold', marginBottom: 4 }}>技能</div>
              <Space wrap>
                {skills.map((s) => (
                  <Tag key={s.skill_name}>{s.skill_name}</Tag>
                ))}
              </Space>
            </div>
          )}
          {agentMd && (
            <div>
              <div style={{ fontWeight: 'bold', marginBottom: 4 }}>Agent 提示词</div>
              <pre
                style={{
                  background: 'var(--color-bg-elevated)',
                  padding: 12,
                  borderRadius: 6,
                  maxHeight: 300,
                  overflow: 'auto',
                  fontSize: 12,
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                }}
              >
                {agentMd}
              </pre>
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
