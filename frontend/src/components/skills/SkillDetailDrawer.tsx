import { useState, useEffect } from 'react';
import { Spin, Drawer, Descriptions, Tag, Alert, Button, Space, message, Modal, Checkbox, Row, Col, Popconfirm } from 'antd';
import Typography from 'antd/es/typography';
import {
  FileTextOutlined, DownloadOutlined, InfoCircleOutlined,
  SwapOutlined, DeleteOutlined, FolderOutlined,
  ArrowLeftOutlined, EyeOutlined,
} from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import * as db from '@/utils/database';
import { formatSize, formatTime, EXECUTOR_COLORS } from './helpers';
import { EXECUTORS, type ExecutorSkills } from '@/types';
import type { SkillMeta } from '@/types';
import type { SkillFileInfo } from '@/utils/database/skills';
import { SkillFileBrowser } from './SkillFileBrowser';
import { SkillFilePreview } from './SkillFilePreview';
import { useTheme } from '@/hooks/useTheme';

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
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';

  const [content, setContent] = useState<string>('');
  const [files, setFiles] = useState<SkillFileInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [syncModalOpen, setSyncModalOpen] = useState(false);
  const [targetExecutors, setTargetExecutors] = useState<string[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [executorsData, setExecutorsData] = useState<ExecutorSkills[]>([]);
  const [selectedFile, setSelectedFile] = useState<SkillFileInfo | null>(null);
  const [fileBrowserOpen, setFileBrowserOpen] = useState(false);

  useEffect(() => {
    if (open && skill) {
      setLoading(true);
      db.getSkillContent(executor, skill.name)
        .then(data => {
          const meta = `# ${data.skill_name}\n\n## 元信息\n- 文件数: ${data.files.length}\n- 大小: ${formatSize(skill.total_size)}\n- 更新时间: ${formatTime(skill.modified_at)}\n\n---\n\n${data.content}`;
          setContent(meta);
          setFiles(data.files);
          // 默认选中 SKILL.md 文件
          const skillMd = data.files.find(f => f.path === 'SKILL.md');
          setSelectedFile(skillMd || data.files[0] || null);
        })
        .catch(() => {
          setContent(`# ${skill.name}\n\n${skill.description || '暂无描述'}\n\n## 元信息\n- 版本: ${skill.version || '未指定'}\n- 作者: ${skill.author || '未知'}\n- 许可证: ${skill.license || '未指定'}\n- 文件数: ${skill.file_count}\n- 大小: ${formatSize(skill.total_size)}\n- 更新时间: ${formatTime(skill.modified_at)}`);
          setFiles([]);
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
    <>
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

            {/* 操作按钮区域 - 放在内容区域内部 */}
            <div style={{
              display: 'flex',
              gap: 8,
              flexWrap: 'wrap',
              marginBottom: 16,
              padding: '12px',
              background: isDark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.02)',
              borderRadius: 8,
              border: `1px solid ${isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)'}`,
            }}>
              <Button
                type="primary"
                icon={<FolderOutlined />}
                onClick={() => setFileBrowserOpen(true)}
                disabled={files.length === 0}
              >
                浏览文件 {files.length > 0 && `(${files.length})`}
              </Button>
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
            </div>

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

            <h3 style={{
              margin: '16px 0 8px',
              color: isDark ? '#e2e8f0' : '#595959',
            }}>内容预览</h3>
            <XMarkdown
              content={content}
              escapeRawHtml={true}
              style={{
                fontFamily: 'Fira Code, monospace',
                fontSize: 13,
                background: isDark ? '#1a1a2e' : '#1e1e1e',
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

      {/* 全屏文件浏览器模态框 */}
      <Modal
        title={
          <Space>
            <FolderOutlined style={{ color: EXECUTOR_COLORS[executor] || '#7C3AED' }} />
            <span>{skill?.name} - 文件浏览</span>
            <Tag color={EXECUTOR_COLORS[executor]}>{executorLabel}</Tag>
            <Tag color="default">{files.length} 个文件</Tag>
          </Space>
        }
        open={fileBrowserOpen}
        onCancel={() => setFileBrowserOpen(false)}
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
        <FileBrowserFullscreen
          files={files}
          loading={loading}
          selectedFile={selectedFile}
          onFileSelect={setSelectedFile}
          executor={executor}
          skillName={skill?.name || ''}
          isDark={isDark}
        />
      </Modal>
    </>
  );
}

// 文件浏览器全屏模态框内容组件，响应式支持手机端切换视图
function FileBrowserFullscreen({
  files,
  loading,
  selectedFile,
  onFileSelect,
  executor,
  skillName,
  isDark,
}: {
  files: SkillFileInfo[];
  loading: boolean;
  selectedFile: SkillFileInfo | null;
  onFileSelect: (file: SkillFileInfo) => void;
  executor: string;
  skillName: string;
  isDark: boolean;
}) {
  // 手机端使用独立状态控制视图模式，避免受 PC 端行为影响
  const [isMobilePreviewMode, setIsMobilePreviewMode] = useState(false);

  // 切换到文件列表视图时重置预览模式
  const handleFileSelect = (file: SkillFileInfo) => {
    onFileSelect(file);
    // 手机端选中文件后自动进入预览模式
    setIsMobilePreviewMode(true);
  };

  // 响应式布局：PC 端保持左右布局，手机端切换为单视图模式
  const isMobile = typeof window !== 'undefined' && window.innerWidth < 768;

  // PC 端或未选中文件时显示左右分栏布局
  if (!isMobile && !isMobilePreviewMode) {
    return (
      <div style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
        {/* 左侧：文件树 */}
        <div style={{
          flex: '0 0 280px',
          borderRight: `1px solid ${isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)'}`,
          overflow: 'auto',
          background: isDark ? '#1a1a2e' : '#fff',
        }}>
          <SkillFileBrowser
            files={files}
            loading={loading}
            onFileSelect={onFileSelect}
            selectedFile={selectedFile}
            isDark={isDark}
          />
        </div>
        {/* 右侧：文件预览 */}
        <div style={{
          flex: 1,
          overflow: 'auto',
          background: isDark ? '#1a1a2e' : '#fff',
        }}>
          <SkillFilePreview
            file={selectedFile}
            executor={executor}
            skillName={skillName}
            isDark={isDark}
          />
        </div>
      </div>
    );
  }

  // 手机端：显示文件列表视图或预览视图（通过按钮切换）
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* 手机端顶部切换按钮栏 */}
      {isMobile && (
        <div style={{
          display: 'flex',
          gap: 8,
          padding: '8px 12px',
          borderBottom: `1px solid ${isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)'}`,
          background: isDark ? 'rgba(255,255,255,0.02)' : 'rgba(0,0,0,0.02)',
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

      {/* 内容区域 */}
      <div style={{ flex: 1, overflow: 'hidden' }}>
        {isMobilePreviewMode ? (
          // 预览视图
          <div style={{ height: '100%', overflow: 'auto' }}>
            {/* 手机端预览顶部导航栏 */}
            {isMobile && (
              <div style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                padding: '8px 12px',
                borderBottom: `1px solid ${isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)'}`,
                background: isDark ? 'rgba(255,255,255,0.02)' : 'rgba(0,0,0,0.02)',
              }}>
                <Button
                  size="small"
                  icon={<ArrowLeftOutlined />}
                  onClick={() => setIsMobilePreviewMode(false)}
                >
                  返回列表
                </Button>
                {selectedFile && (
                  <Text style={{ fontSize: 13, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {selectedFile.path}
                  </Text>
                )}
              </div>
            )}
            <SkillFilePreview
              file={selectedFile}
              executor={executor}
              skillName={skillName}
              isDark={isDark}
            />
          </div>
        ) : (
          // 文件列表视图
          <div style={{ height: '100%', overflow: 'auto' }}>
            <SkillFileBrowser
              files={files}
              loading={loading}
              onFileSelect={handleFileSelect}
              selectedFile={selectedFile}
              isDark={isDark}
            />
          </div>
        )}
      </div>
    </div>
  );
}
