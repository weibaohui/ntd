import { useState, useEffect } from 'react';
import { Spin, Drawer, Descriptions, Tag, Alert, Button, Space, message, Modal, Checkbox, Row, Col, Popconfirm } from 'antd';
import Typography from 'antd/es/typography';
import { FileTextOutlined, DownloadOutlined, InfoCircleOutlined, SwapOutlined, DeleteOutlined } from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import * as db from '@/utils/database';
import { formatSize, formatTime, EXECUTOR_COLORS } from './helpers';
import { EXECUTORS, type ExecutorSkills } from '@/types';
import type { SkillMeta } from '@/types';

const { Text } = Typography;

interface SkillDetailDrawerProps {
  skill: SkillMeta | null;
  executor: string;
  executorLabel: string;
  open: boolean;
  onClose: () => void;
  onSyncSuccess?: () => void;
  onDeleteSuccess?: () => void;
}

export function SkillDetailDrawer({ skill, executor, executorLabel, open, onClose, onSyncSuccess, onDeleteSuccess }: SkillDetailDrawerProps) {
  const [content, setContent] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [syncModalOpen, setSyncModalOpen] = useState(false);
  const [targetExecutors, setTargetExecutors] = useState<string[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [executorsData, setExecutorsData] = useState<ExecutorSkills[]>([]);

  useEffect(() => {
    if (open && skill) {
      setLoading(true);
      db.getSkillContent(executor, skill.name)
        .then(data => {
          const meta = `# ${data.skill_name}\n\n## 元信息\n- 文件数: ${data.files.length}\n- 大小: ${formatSize(skill.total_size)}\n- 更新时间: ${formatTime(skill.modified_at)}\n\n---\n\n${data.content}`;
          setContent(meta);
        })
        .catch(() => {
          setContent(`# ${skill.name}\n\n${skill.description || '暂无描述'}\n\n## 元信息\n- 版本: ${skill.version || '未指定'}\n- 作者: ${skill.author || '未知'}\n- 许可证: ${skill.license || '未指定'}\n- 文件数: ${skill.file_count}\n- 大小: ${formatSize(skill.total_size)}\n- 更新时间: ${formatTime(skill.modified_at)}`);
        })
        .finally(() => setLoading(false));
    }
  }, [open, skill, executor]);

  const handleOpenSyncModal = () => {
    setTargetExecutors([]);
    db.getSkillsList()
      .then(data => setExecutorsData(data.filter(e => e.skills_dir_exists)))
      .catch(() => setExecutorsData([]))
      .finally(() => setSyncModalOpen(true));
  };

  const handleSync = async () => {
    if (!skill || targetExecutors.length === 0) return;
    setSyncing(true);
    try {
      const result = await db.syncSkill(executor, skill.name, targetExecutors);
      message.success(result || '同步完成');
      setSyncModalOpen(false);
      setTargetExecutors([]);
      onSyncSuccess?.();
    } catch (err: any) {
      message.error('同步失败: ' + (err?.message || String(err)));
    } finally {
      setSyncing(false);
    }
  };

  const handleExport = async () => {
    if (!skill) return;
    try {
      const blob = await db.exportSkill(executor, skill.name);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${skill.name}.zip`;
      a.click();
      URL.revokeObjectURL(url);
      message.success(`导出 ${skill.name} 成功`);
    } catch {
      message.error(`导出 ${skill.name} 失败`);
    }
  };

  const handleDelete = async () => {
    if (!skill) return;
    try {
      await db.deleteSkill(executor, skill.name);
      message.success(`已删除 ${skill.name}`);
      onClose();
      onDeleteSuccess?.();
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

  return (
    <Drawer
      title={
        <Space>
          <FileTextOutlined style={{ color: EXECUTOR_COLORS[executor] || '#7C3AED' }} />
          <span>{skill?.name || 'Skill 详情'}</span>
          <Tag color={EXECUTOR_COLORS[executor]}>{executorLabel}</Tag>
        </Space>
      }
      placement="right"
      width={640}
      onClose={onClose}
      open={open}
      extra={
        <Space>
          <Button icon={<SwapOutlined />} onClick={handleOpenSyncModal}>
            同步
          </Button>
          <Button icon={<DownloadOutlined />} onClick={handleExport}>
            导出
          </Button>
          <Popconfirm
            title="删除 Skill"
            description={`确定要删除 ${executorLabel} 下的「${skill?.name}」吗？此操作不可恢复。`}
            onConfirm={handleDelete}
            okText="删除"
            cancelText="取消"
            okButtonProps={{ danger: true }}
          >
            <Button icon={<DeleteOutlined />} danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      }
    >
      {loading ? (
        <div style={{ textAlign: 'center', padding: 40 }}>
          <Spin size="large" />
        </div>
      ) : (
        <div>
          {skill?.description && (
            <Alert
              message={skill.description}
              type="info"
              showIcon
              icon={<InfoCircleOutlined />}
              style={{ marginBottom: 16 }}
            />
          )}

          <Descriptions bordered size="small" column={2}>
            <Descriptions.Item label="版本">
              {skill?.version ? <Tag color="blue">{skill.version}</Tag> : <Text type="secondary">未指定</Text>}
            </Descriptions.Item>
            <Descriptions.Item label="作者">
              {skill?.author || <Text type="secondary">未知</Text>}
            </Descriptions.Item>
            <Descriptions.Item label="许可证">
              {skill?.license || <Text type="secondary">未指定</Text>}
            </Descriptions.Item>
            <Descriptions.Item label="文件数">
              {skill?.file_count || 0}
            </Descriptions.Item>
            <Descriptions.Item label="大小" span={2}>
              {formatSize(skill?.total_size || 0)}
            </Descriptions.Item>
            <Descriptions.Item label="更新时间" span={2}>
              {formatTime(skill?.modified_at ?? null)}
            </Descriptions.Item>
            {skill?.keywords && skill.keywords.length > 0 && (
              <Descriptions.Item label="关键词" span={2}>
                {skill.keywords.map(k => (
                  <Tag key={k} color="purple" style={{ marginBottom: 4 }}>{k}</Tag>
                ))}
              </Descriptions.Item>
            )}
          </Descriptions>

          <h3 style={{ margin: '16px 0 8px', color: '#595959' }}>内容预览</h3>
          <XMarkdown
            content={content}
            escapeRawHtml={true}
            style={{
              fontFamily: 'Fira Code, monospace',
              fontSize: 13,
              background: '#1e1e1e',
              color: '#d4d4d4',
              padding: '12px',
              borderRadius: '8px',
            }}
          />
        </div>
      )}
      <Modal
        title={
          <Space>
            <SwapOutlined style={{ color: '#7C3AED' }} />
            <span>同步 Skill 到其他执行器</span>
          </Space>
        }
        open={syncModalOpen}
        onCancel={() => setSyncModalOpen(false)}
        onOk={handleSync}
        okText={`同步到 ${targetExecutors.length} 个执行器`}
        okButtonProps={{ disabled: targetExecutors.length === 0 }}
        confirmLoading={syncing}
        width={480}
      >
        <div style={{ marginBottom: 16 }}>
          <Text type="secondary">
            将 <Text strong>{skill?.name}</Text> 从 <Tag color={EXECUTOR_COLORS[executor]}>{executorLabel}</Tag> 复制到以下执行器：
          </Text>
        </div>
        <Checkbox.Group
          value={targetExecutors}
          onChange={v => setTargetExecutors(v as string[])}
          style={{ width: '100%' }}
        >
          <Row gutter={[8, 8]}>
            {EXECUTORS.filter(e => e.value !== executor).map(exec => {
              const exists = executorsData.find(ex => ex.executor === exec.value);
              const targetName = skill?.name?.split('/').pop() || skill?.name;
              const alreadyHas = exists?.skills.find(s => s.name === targetName);
              // agents 是只读来源，不能作为同步目标
              const isReadonly = exec.value === 'agents';
              return (
                <Col span={12} key={exec.value}>
                  <Checkbox value={exec.value} disabled={isReadonly}>
                    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                      <span style={{
                        width: 6, height: 6, borderRadius: '50%',
                        backgroundColor: exec.color,
                      }} />
                      {exec.label}
                      {isReadonly && <Tag color="default" style={{ fontSize: 10 }}>只读</Tag>}
                      {alreadyHas && <Tag color="orange" style={{ fontSize: 10 }}>已存在</Tag>}
                    </span>
                  </Checkbox>
                </Col>
              );
            })}
          </Row>
        </Checkbox.Group>
      </Modal>
    </Drawer>
  );
}
