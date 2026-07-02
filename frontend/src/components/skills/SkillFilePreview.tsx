import { useState, useEffect } from 'react';
import { Spin, Typography, Tag, Space, Button, message } from 'antd';
import {
  DownloadOutlined, CopyOutlined, FileOutlined,
} from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import type { SkillFileInfo } from '@/utils/database/skills';
import { getSkillContent, getSkillFileContent } from '@/utils/database/skills';
import { formatSize, formatTime } from './helpers';

const { Text } = Typography;

interface SkillFilePreviewProps {
  file: SkillFileInfo | null;
  executor: string;
  skillName: string;
  loading?: boolean;
  isDark?: boolean;
}

// 获取文件扩展名对应的图标颜色
function getFileColor(filename: string, isDark?: boolean): string {
  const ext = filename.split('.').pop()?.toLowerCase();
  const colorMap: Record<string, string> = {
    md: '#0891b2',
    ts: '#3178c6',
    tsx: '#3178c6',
    js: '#f7df1e',
    jsx: '#f7df1e',
    json: '#f59e0b',
    yaml: '#e11d48',
    yml: '#e11d48',
    toml: '#9333ea',
    txt: isDark ? '#94a3b8' : '#64748b',
    css: '#06b6d4',
    html: '#ea580c',
  };
  return colorMap[ext || ''] || (isDark ? '#94a3b8' : '#64748b');
}

export function SkillFilePreview({ file, executor, skillName, loading, isDark }: SkillFilePreviewProps) {
  const [content, setContent] = useState<string>('');
  const [contentLoading, setContentLoading] = useState(false);

  // 加载文件内容
  useEffect(() => {
    if (!file) {
      setContent('');
      return;
    }

    // 对于 SKILL.md 文件，使用主内容
    if (file.path === 'SKILL.md') {
      setContentLoading(true);
      getSkillContent(executor, skillName)
        .then(data => setContent(data.content))
        .catch(() => setContent('无法加载文件内容'))
        .finally(() => setContentLoading(false));
      return;
    }

    // 对于其他文件，通过 API 获取单文件内容
    setContentLoading(true);
    getSkillFileContent(executor, skillName, file.path)
      .then(data => setContent(data.content))
      .catch(() => setContent(`# ${file.path}\n\n文件信息：\n- 大小: ${formatSize(file.size)}\n- 修改时间: ${formatTime(file.modified_at)}\n\n---\n\n[无法加载文件内容]`))
      .finally(() => setContentLoading(false));
  }, [file, executor, skillName]);

  // 主题相关颜色
  const bgColor = isDark ? '#1a1a2e' : '#1e1e1e';
  const headerBg = isDark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.02)';
  const borderColor = isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)';
  const secondaryColor = isDark ? '#94a3b8' : '#64748b';

  if (!file) {
    return (
      <div style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100%',
        color: secondaryColor,
      }}>
        <FileOutlined style={{ fontSize: 48, marginBottom: 16, opacity: 0.5 }} />
        <Text style={{ color: secondaryColor }}>选择一个文件查看内容</Text>
      </div>
    );
  }

  if (contentLoading || loading) {
    return (
      <div style={{ textAlign: 'center', padding: 40 }}>
        <Spin size="large" />
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* 文件头部信息 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '8px 12px',
        background: headerBg,
        borderRadius: '8px 8px 0 0',
        borderBottom: `1px solid ${borderColor}`,
      }}>
        <Space size={8}>
          <FileOutlined style={{ color: getFileColor(file.path, isDark) }} />
          <Text strong style={{ fontSize: 13 }}>{file.path}</Text>
          <Tag color="default" style={{ fontSize: 11, lineHeight: '16px', padding: '0 6px' }}>
            {formatSize(file.size)}
          </Tag>
        </Space>
        <Space size={4}>
          <Button
            type="text"
            size="small"
            icon={<CopyOutlined />}
            onClick={() => {
              navigator.clipboard.writeText(content);
              message.success('已复制到剪贴板');
            }}
          />
          <Button
            type="text"
            size="small"
            icon={<DownloadOutlined />}
            onClick={() => {
              const blob = new Blob([content], { type: 'text/plain' });
              const url = URL.createObjectURL(blob);
              const a = document.createElement('a');
              a.href = url;
              a.download = file.path.split('/').pop() || 'file';
              a.click();
              URL.revokeObjectURL(url);
            }}
          />
        </Space>
      </div>

      {/* 文件内容 */}
      <div style={{
        flex: 1,
        overflow: 'auto',
        background: bgColor,
        borderRadius: '0 0 8px 8px',
      }}>
        {file.path.endsWith('.md') ? (
          <XMarkdown
            content={content}
            escapeRawHtml={true}
            style={{
              fontFamily: 'Fira Code, monospace',
              fontSize: 13,
              color: '#d4d4d4',
              padding: '12px',
            }}
          />
        ) : (
          <pre style={{
            fontFamily: 'Fira Code, monospace',
            fontSize: 13,
            color: '#d4d4d4',
            padding: '12px',
            margin: 0,
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}>
            {content}
          </pre>
        )}
      </div>
    </div>
  );
}
