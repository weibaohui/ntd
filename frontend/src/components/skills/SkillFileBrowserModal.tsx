/**
 * 文件浏览器全屏 Modal（PC + Mobile 响应式）
 *
 * 从 SkillDetailDrawer 抽出，结构对外暴露：
 * - props.files：扁平文件列表（path/size）
 * - props.skillName / props.title：用于 Modal 标题
 * - props.loadContent：父组件决定怎么读文件内容（适配 executor 系或 bundled 系）
 * - props.presetContent / props.presetPath：可选的预设内容（避免 SKILL.md 重复请求）
 *
 * 内部仍复用 SkillFileBrowser（左侧/手机端文件树）+ SkillFilePreview（预览）。
 * Mobile 端通过顶部「文件列表/预览」按钮切换视图。
 */
import { useState } from 'react';
import { Modal, Button, Space, Tag, Typography } from 'antd';
import { FolderOutlined, EyeOutlined, ArrowLeftOutlined } from '@ant-design/icons';
import { SkillFileBrowser } from './SkillFileBrowser';
import { SkillFilePreview } from './SkillFilePreview';
import type { SkillFileInfo } from '@/utils/database/skills';

const { Text } = Typography;

interface SkillFileBrowserModalProps {
  open: boolean;
  onClose: () => void;
  /** Modal 标题前缀，例：「api-and-interface-design」 */
  title: string;
  /** Tag 标签文字，例：「claudecode」或「Addy Osmani Agent Skills」 */
  badgeLabel?: string;
  files: SkillFileInfo[];
  loading?: boolean;
  presetContent?: string;
  presetPath?: string;
  loadContent: (file: SkillFileInfo) => Promise<string>;
}

export function SkillFileBrowserModal({
  open,
  onClose,
  title,
  badgeLabel,
  files,
  loading,
  presetContent,
  presetPath,
  loadContent,
}: SkillFileBrowserModalProps) {
  const [selectedFile, setSelectedFile] = useState<SkillFileInfo | null>(
    // 默认选中 SKILL.md（如有），与 SkillDetailDrawer 行为一致
    () => files.find(f => f.path === 'SKILL.md') || files[0] || null
  );
  // 手机端视图模式：false=文件列表；true=预览
  const [isMobilePreviewMode, setIsMobilePreviewMode] = useState(false);

  // 响应式判断：构造时取一次窗口宽度即可（不需要 resize 监听——drawer/modal 本身会
  // 处理宽高变化，这种 file browser 在小窗下用户体验差异不大）
  const isMobile = typeof window !== 'undefined' && window.innerWidth < 768;

  const handleFileSelect = (file: SkillFileInfo) => {
    setSelectedFile(file);
    setIsMobilePreviewMode(true);
  };

  return (
    <Modal
      title={
        <Space>
          <FolderOutlined style={{ color: 'var(--color-primary)' }} />
          <span style={{ color: 'var(--color-text)' }}>{title} - 文件浏览</span>
          {badgeLabel && (
            <Tag
              style={{
                background: 'var(--color-bg-tertiary)',
                color: 'var(--color-text-secondary)',
                border: 'none',
              }}
            >
              {badgeLabel}
            </Tag>
          )}
          <Tag
            style={{
              background: 'var(--color-bg-tertiary)',
              color: 'var(--color-text-secondary)',
              border: 'none',
            }}
          >
            {files.length} 个文件
          </Tag>
        </Space>
      }
      open={open}
      onCancel={onClose}
      footer={null}
      width="90vw"
      style={{ top: 20 }}
      styles={{
        body: {
          height: 'calc(100vh - 100px)',
          padding: 0,
          display: 'flex',
          flexDirection: 'column',
        },
      }}
    >
      {/* PC 端：左右分栏；Mobile 端：单视图+顶部切换 */}
      {!isMobile && !isMobilePreviewMode ? (
        <div style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
          {/* 左侧：文件树 */}
          <div style={{
            flex: '0 0 280px',
            borderRight: '1px solid var(--color-border-light)',
            overflow: 'auto',
            background: 'var(--color-bg-elevated)',
          }}>
            <SkillFileBrowser
              files={files}
              loading={loading}
              onFileSelect={setSelectedFile}
              selectedFile={selectedFile}
            />
          </div>
          {/* 右侧：预览 */}
          <div style={{
            flex: 1,
            overflow: 'auto',
            background: 'var(--color-bg-elevated)',
          }}>
            <SkillFilePreview
              file={selectedFile}
              loadContent={loadContent}
              presetContent={presetContent}
              presetPath={presetPath}
            />
          </div>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
          {/* Mobile 端顶部切换栏 */}
          {isMobile && (
            <div style={{
              display: 'flex',
              gap: 8,
              padding: '8px 12px',
              borderBottom: '1px solid var(--color-border-light)',
              background: 'var(--color-bg-card)',
            }}>
              <Button
                size="small"
                icon={<FolderOutlined />}
                type={!isMobilePreviewMode ? 'primary' : 'default'}
                onClick={() => setIsMobilePreviewMode(false)}
              >
                文件列表
              </Button>
              <Button
                size="small"
                icon={<EyeOutlined />}
                type={isMobilePreviewMode ? 'primary' : 'default'}
                onClick={() => setIsMobilePreviewMode(true)}
                disabled={!selectedFile}
              >
                预览
              </Button>
            </div>
          )}

          <div style={{ flex: 1, overflow: 'hidden' }}>
            {isMobilePreviewMode ? (
              <div style={{ height: '100%', overflow: 'auto' }}>
                {isMobile && (
                  <div style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 8,
                    padding: '8px 12px',
                    borderBottom: '1px solid var(--color-border-light)',
                    background: 'var(--color-bg-card)',
                  }}>
                    <Button
                      size="small"
                      icon={<ArrowLeftOutlined />}
                      onClick={() => setIsMobilePreviewMode(false)}
                    >
                      返回列表
                    </Button>
                    {selectedFile && (
                      <Text style={{
                        fontSize: 13,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                        color: 'var(--color-text)',
                      }}>
                        {selectedFile.path}
                      </Text>
                    )}
                  </div>
                )}
                <SkillFilePreview
                  file={selectedFile}
                  loadContent={loadContent}
                  presetContent={presetContent}
                  presetPath={presetPath}
                />
              </div>
            ) : (
              <div style={{ height: '100%', overflow: 'auto' }}>
                <SkillFileBrowser
                  files={files}
                  loading={loading}
                  onFileSelect={handleFileSelect}
                  selectedFile={selectedFile}
                />
              </div>
            )}
          </div>
        </div>
      )}
    </Modal>
  );
}
