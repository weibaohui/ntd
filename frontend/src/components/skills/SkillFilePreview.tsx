import { useState, useEffect } from 'react';
import { Spin, Typography, Tag, Space, Button, message } from 'antd';
import {
  DownloadOutlined, CopyOutlined, FileOutlined,
} from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import type { SkillFileInfo } from '@/utils/database/skills';
import { formatSize, formatTime, getFileColor } from './helpers';

const { Text } = Typography;

interface SkillFilePreviewProps {
  file: SkillFileInfo | null;
  /**
   * 加载文件内容：父组件决定怎么取——
   * - 已安装技能：调 db.getSkillFileContent(executor, skillName, file.path)
   * - 市场 bundled 技能：path === 'SKILL.md' 时返回缓存内容，其他文件返回不支持
   */
  loadContent: (file: SkillFileInfo) => Promise<string>;
  /** 父组件已缓存的 SKILL.md 等文件类型，用于"主内容"路径——比如 SKILL.md 直接读这个，省一次 API */
  presetContent?: string;
  presetPath?: string;
  loading?: boolean;
  isDark?: boolean;
}

export function SkillFilePreview({
  file,
  loadContent,
  presetContent,
  presetPath,
  loading,
  isDark,
}: SkillFilePreviewProps) {
  const [content, setContent] = useState<string>('');
  const [contentLoading, setContentLoading] = useState(false);

  // 加载文件内容——优先用 presetContent（父组件已知的内容），否则调 loadContent
  useEffect(() => {
    if (!file) {
      setContent('');
      return;
    }

    // 预设命中：路径一致且有缓存，直接用，避免重复请求
    if (presetPath && presetContent && file.path === presetPath) {
      setContent(presetContent);
      return;
    }

    let cancelled = false;
    setContentLoading(true);
    loadContent(file)
      .then(data => {
        if (!cancelled) setContent(data);
      })
      .catch(err => {
        if (!cancelled) {
          setContent(
            `# ${file.path}\n\n文件信息：\n- 大小: ${formatSize(file.size)}\n- 修改时间: ${formatTime(file.modified_at ?? null)}\n\n---\n\n[无法加载文件内容: ${err?.message || String(err)}]`
          );
        }
      })
      .finally(() => {
        if (!cancelled) setContentLoading(false);
      });
    return () => { cancelled = true; };
  }, [file, loadContent, presetContent, presetPath]);

  // 主题相关颜色（用 CSS 变量，亮/暗双主题自动跟随）
  const bgColor = 'var(--color-bg)';
  const headerBg = 'var(--color-bg-card)';
  const borderColor = 'var(--color-border-light)';
  const secondaryColor = 'var(--color-text-tertiary)';

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
          <Text strong style={{ fontSize: 13, color: 'var(--color-text)' }}>{file.path}</Text>
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
              navigator.clipboard.writeText(content)
                .then(() => message.success('已复制到剪贴板'))
                .catch(() => message.error('复制失败'));
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
        color: 'var(--color-text)',
      }}>
        {file.path.endsWith('.md') ? (
          <XMarkdown
            content={content}
            escapeRawHtml={true}
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: 13,
              color: 'var(--color-text)',
              padding: '12px',
            }}
          />
        ) : (
          <pre style={{
            fontFamily: 'var(--font-mono)',
            fontSize: 13,
            color: 'var(--color-text)',
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
