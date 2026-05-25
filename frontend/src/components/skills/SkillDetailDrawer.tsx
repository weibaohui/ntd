import { useState, useEffect } from 'react';
import { Spin, Drawer, Descriptions, Tag, Alert, Button, Space, message } from 'antd';
import Typography from 'antd/es/typography';
import { FileTextOutlined, DownloadOutlined, InfoCircleOutlined } from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import * as db from '../../utils/database';
import { formatSize, formatTime, EXECUTOR_COLORS } from './helpers';
import type { SkillMeta } from '../../types';

const { Text } = Typography;

interface SkillDetailDrawerProps {
  skill: SkillMeta | null;
  executor: string;
  executorLabel: string;
  open: boolean;
  onClose: () => void;
}

export function SkillDetailDrawer({ skill, executor, executorLabel, open, onClose }: SkillDetailDrawerProps) {
  const [content, setContent] = useState<string>('');
  const [loading, setLoading] = useState(false);

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
          <Button icon={<DownloadOutlined />} onClick={handleExport}>
            导出
          </Button>
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
    </Drawer>
  );
}
