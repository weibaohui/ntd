import { useState, useEffect, useMemo, useCallback, type ReactNode } from 'react';
import { useIsMobile } from '../hooks/useIsMobile';
import {
  Card,
  Table,
  Tag,
  Spin,
  Empty,
  Space,
  Typography,
  Tooltip,
  Badge,
  Button,
  Checkbox,
  message,
  Input,
  Select,
  Statistic,
  Row,
  Col,
  Modal,
  Drawer,
  Dropdown,
  Alert,
  List,
  Descriptions,
  Upload,
} from 'antd';
import type { MenuProps } from 'antd';
import {
  ThunderboltOutlined,
  SwapOutlined,
  BarChartOutlined,
  AppstoreOutlined,
  CheckCircleOutlined,
  CopyOutlined,
  ReloadOutlined,
  SearchOutlined,
  DownloadOutlined,
  UploadOutlined,
  FolderOutlined,
  FileOutlined,
  FolderOpenOutlined,
  ExportOutlined,
  ImportOutlined,
  InfoCircleOutlined,
  CaretRightOutlined,
  CaretDownOutlined,
  FileTextOutlined,
  SaveOutlined,
} from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import * as db from '../utils/database';
import type { ExecutorSkills, SkillComparison, SkillInvocation, SkillMeta } from '../types';
import { EXECUTORS } from '../types';

const { Text, Paragraph } = Typography;

// ── 工具函数 ────────────────────────────────────────────────

const EXECUTOR_COLORS: Record<string, string> = {};
EXECUTORS.forEach(e => { EXECUTOR_COLORS[e.value] = e.color; });
// Aliases for backward compatibility
EXECUTOR_COLORS['claude_code'] = EXECUTOR_COLORS['claudecode'];
EXECUTOR_COLORS['claude'] = EXECUTOR_COLORS['claudecode'];
EXECUTOR_COLORS['cbc'] = EXECUTOR_COLORS['codebuddy'];
EXECUTOR_COLORS['atom'] = EXECUTOR_COLORS['atomcode'];

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatTime(iso: string | null): string {
  if (!iso) return '-';
  try {
    const d = new Date(iso);
    return d.toLocaleDateString('zh-CN') + ' ' + d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
  } catch {
    return iso;
  }
}

// ── 类型定义 ────────────────────────────────────────────────

interface SkillTreeNode {
  key: string;
  name: string;
  type: 'category' | 'skill';
  executor: string;
  color: string;
  data: SkillMeta | null;
  children?: SkillTreeNode[];
  depth: number;
}

interface ExportTask {
  id: string;
  executor: string;
  skillName: string;
  status: 'pending' | 'exporting' | 'completed' | 'failed';
  progress: number;
  error?: string;
  blobUrl?: string;
}

// ── Skill 详情抽屉 ──────────────────────────────────────────

interface SkillDetailDrawerProps {
  skill: SkillMeta | null;
  executor: string;
  executorLabel: string;
  open: boolean;
  onClose: () => void;
}

function SkillDetailDrawer({ skill, executor, executorLabel, open, onClose }: SkillDetailDrawerProps) {
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

// ── 导入导出 Modal ───────────────────────────────────────────

// 规范化执行器名称：统一转换为小写，用于匹配
function normalizeExecutor(name: string): string {
  return name.toLowerCase().replace(/[_\s-]/g, '');
}

interface ImportExportModalProps {
  open: boolean;
  mode: 'import' | 'export';
  executor: string;
  data: ExecutorSkills[];
  initialSelectedSkills?: string[];
  onClose: () => void;
}

function ImportExportModal({ open, mode, executor, data, initialSelectedSkills, onClose }: ImportExportModalProps) {
  const [selectedSkills, setSelectedSkills] = useState<string[]>(initialSelectedSkills || []);
  const [exporting, setExporting] = useState(false);
  const [tasks, setTasks] = useState<ExportTask[]>([]);
  const [importFile, setImportFile] = useState<File | null>(null);
  const [importing, setImporting] = useState(false);

  // 使用规范化匹配执行器（前后端执行器名称格式可能不一致）
  const executorData = useMemo(() => {
    const normalized = normalizeExecutor(executor);
    // 优先精确匹配
    let found = data.find(e => e.executor === executor);
    if (!found) {
      // 尝试规范化后匹配（处理 claude_code vs claudecode 等情况）
      found = data.find(e => normalizeExecutor(e.executor) === normalized);
    }
    if (!found && data.length > 0) {
      // 如果都匹配不上，使用传入的 executor 作为 fallback
      found = { executor, executor_label: executor, skills: [], skills_dir: '', skills_dir_exists: false };
    }
    return found;
  }, [data, executor]);
  const skills = executorData?.skills || [];

  // 每次模态框打开时，重置选中状态
  useEffect(() => {
    if (open) {
      setSelectedSkills(initialSelectedSkills || []);
      setTasks([]);
    }
  }, [open, initialSelectedSkills]);
  // Helper to revoke all blob URLs from tasks
  const revokeTasksBlobUrls = useCallback((taskList: ExportTask[]) => {
    taskList.forEach(t => { if (t.blobUrl) URL.revokeObjectURL(t.blobUrl); });
  }, []);

  // Cleanup blob URLs when modal closes
  useEffect(() => {
    if (!open) {
      setTasks(prev => {
        revokeTasksBlobUrls(prev);
        return [];
      });
      setSelectedSkills([]);
    }
  }, [open, revokeTasksBlobUrls]);

  const handleExport = async () => {
    if (selectedSkills.length === 0) {
      message.warning('请选择要导出的 Skills');
      return;
    }
    setExporting(true);
    // Revoke old blob URLs before starting new export
    setTasks(prev => {
      revokeTasksBlobUrls(prev);
      return prev;
    });
    const newTasks: ExportTask[] = selectedSkills.map(s => ({
      id: `${Date.now()}-${s}`,
      executor,
      skillName: s,
      status: 'pending',
      progress: 0,
    }));
    setTasks(newTasks);

    let successCount = 0;
    let failCount = 0;

    for (const task of newTasks) {
      setTasks(prev => prev.map(t =>
        t.id === task.id ? { ...t, status: 'exporting' } : t
      ));

      try {
        const blob = await db.exportSkill(task.executor, task.skillName);
        const blobUrl = URL.createObjectURL(blob);

        setTasks(prev => prev.map(t =>
          t.id === task.id ? { ...t, status: 'completed', progress: 100, blobUrl } : t
        ));

        const a = document.createElement('a');
        a.href = blobUrl;
        a.download = `${task.skillName}.zip`;
        a.click();
        successCount++;
      } catch (err: any) {
        setTasks(prev => prev.map(t =>
          t.id === task.id ? { ...t, status: 'failed', error: err.message } : t
        ));
        failCount++;
      }
    }
    setExporting(false);
    if (failCount === 0) {
      message.success(`成功导出 ${successCount} 个 Skills`);
    } else if (successCount === 0) {
      message.error(`导出失败，共 ${failCount} 个 Skills`);
    } else {
      message.info(`导出完成: ${successCount} 个成功, ${failCount} 个失败`);
    }
  };

  const handleImport = async () => {
    if (!importFile) {
      message.warning('请选择要导入的文件');
      return;
    }
    setImporting(true);
    try {
      const result = await db.importSkill(executor, importFile);
      message.success(`导入成功: ${result.skill_name}，共 ${result.imported_files} 个文件`);
      setImportFile(null);
      onClose();
    } catch {
      message.error('导入失败');
    } finally {
      setImporting(false);
    }
  };

  const completedCount = tasks.filter(t => t.status === 'completed').length;

  return (
    <Modal
      title={
        <Space>
          {mode === 'export' ? <ExportOutlined /> : <ImportOutlined />}
          <span>{mode === 'export' ? '导出 Skills' : '导入 Skills'}</span>
        </Space>
      }
      open={open}
      onCancel={onClose}
      width={600}
      footer={
        mode === 'export' ? (
          <Space>
            <Button onClick={onClose}>取消</Button>
            <Button
              type="primary"
              icon={<DownloadOutlined />}
              onClick={handleExport}
              loading={exporting}
              disabled={selectedSkills.length === 0}
            >
              导出 ({selectedSkills.length})
            </Button>
          </Space>
        ) : (
          <Space>
            <Button onClick={onClose}>取消</Button>
            <Button
              type="primary"
              icon={<UploadOutlined />}
              onClick={handleImport}
              loading={importing}
              disabled={!importFile}
            >
              导入
            </Button>
          </Space>
        )
      }
    >
      {mode === 'export' ? (
        <div>
          <Alert
            message="导出说明"
            description="导出的文件为 .zip 压缩包格式，包含 SKILL.md 和所有相关文件。导出后可导入到其他支持 Skills 的应用。"
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
          />

          {tasks.length > 0 ? (
            <List
              size="small"
              dataSource={tasks}
              renderItem={task => (
                <List.Item>
                  <Space style={{ width: '100%' }}>
                    <Text>{task.skillName}</Text>
                    <Tag color={
                      task.status === 'completed' ? 'success' :
                      task.status === 'failed' ? 'error' :
                      task.status === 'exporting' ? 'processing' : 'default'
                    }>
                      {task.status === 'completed' ? '完成' :
                       task.status === 'failed' ? '失败' :
                       task.status === 'exporting' ? `${task.progress}%` : '等待'}
                    </Tag>
                    {task.status === 'completed' && task.blobUrl && (
                      <Button type="link" size="small" icon={<SaveOutlined />} onClick={() => {
                        const a = document.createElement('a');
                        a.href = task.blobUrl!;
                        a.download = `${task.skillName}.zip`;
                        a.click();
                      }}>
                        保存
                      </Button>
                    )}
                  </Space>
                </List.Item>
              )}
            />
          ) : (
            <div style={{ marginBottom: 16 }}>
              <Checkbox.Group
                value={selectedSkills}
                onChange={v => setSelectedSkills(v as string[])}
                style={{ width: '100%' }}
              >
                <Row gutter={[8, 8]}>
                  {skills.length > 0 ? (
                    skills.map(skill => (
                      <Col span={12} key={skill.name}>
                        <Checkbox value={skill.name}>
                          <Text ellipsis style={{ maxWidth: 200 }}>{skill.name}</Text>
                        </Checkbox>
                      </Col>
                    ))
                  ) : (
                    <Col span={24}>
                      <Text type="secondary">该执行器暂无 Skills</Text>
                    </Col>
                  )}
                </Row>
              </Checkbox.Group>
            </div>
          )}

          {tasks.length > 0 && completedCount === tasks.length && (
            <Alert
              message={`成功导出 ${completedCount} 个 Skills`}
              type="success"
              showIcon
              style={{ marginTop: 16 }}
            />
          )}
        </div>
      ) : (
        <div>
          <Alert
            message="导入说明"
            description="支持导入 .zip 压缩包格式的 Skills。导入时可根据目标应用自动处理目录层级。"
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
          />
          <Upload.Dragger
            accept=".zip"
            beforeUpload={(file) => {
              setImportFile(file);
              return false;
            }}
          >
            <p className="ant-upload-drag-icon">
              <UploadOutlined style={{ fontSize: 48, color: '#7C3AED' }} />
            </p>
            <p className="ant-upload-text">点击或拖拽上传 Skills 压缩包</p>
            <p className="ant-upload-hint">支持 .zip 格式</p>
          </Upload.Dragger>
        </div>
      )}
    </Modal>
  );
}

// ── Skill 树形列表 ──────────────────────────────────────────

interface SkillTreeProps {
  data: ExecutorSkills[];
  onSkillClick: (skill: SkillMeta, executor: string) => void;
  onImport: (executor: string) => void;
  onExport: (executor: string, all?: boolean) => void;
  searchText: string;
  showCategory: boolean;
}

function SkillTree({ data, onSkillClick, onImport, onExport, searchText, showCategory }: SkillTreeProps) {
  const [expandedKeys, setExpandedKeys] = useState<string[]>([]);

  const buildTree = useCallback((executorData: ExecutorSkills): SkillTreeNode[] => {
    const nodes: SkillTreeNode[] = [];
    const lowerSearch = searchText.toLowerCase();

    executorData.skills.forEach(skill => {
      // 搜索过滤
      if (lowerSearch && !skill.name.toLowerCase().includes(lowerSearch) &&
          !skill.description?.toLowerCase().includes(lowerSearch)) {
        return;
      }

      if (showCategory && skill.name.includes('/')) {
        // 两级目录结构
        const [category, ...rest] = skill.name.split('/');
        const skillName = rest.join('/');

        let categoryNode = nodes.find(n => n.name === category && n.type === 'category');
        if (!categoryNode) {
          categoryNode = {
            key: `${executorData.executor}-${category}`,
            name: category,
            type: 'category',
            executor: executorData.executor,
            color: EXECUTOR_COLORS[executorData.executor] || '#7C3AED',
            data: null,
            children: [],
            depth: 0,
          };
          nodes.push(categoryNode);
        }

        categoryNode.children!.push({
          key: `${executorData.executor}-${skill.name}`,
          name: skillName,
          type: 'skill',
          executor: executorData.executor,
          color: EXECUTOR_COLORS[executorData.executor] || '#7C3AED',
          data: skill,
          depth: 1,
        });
      } else {
        // 一级结构
        nodes.push({
          key: `${executorData.executor}-${skill.name}`,
          name: skill.name,
          type: 'skill',
          executor: executorData.executor,
          color: EXECUTOR_COLORS[executorData.executor] || '#7C3AED',
          data: skill,
          depth: 0,
        });
      }
    });

    return nodes;
  }, [searchText, showCategory]);

  const renderNode = (node: SkillTreeNode) => {
    if (node.type === 'category') {
      const isExpanded = expandedKeys.includes(node.key);
      return (
        <div key={node.key}>
          <div
            style={{
              padding: '8px 12px',
              marginBottom: 4,
              borderRadius: 6,
              background: `${node.color}10`,
              borderLeft: `3px solid ${node.color}`,
              cursor: 'pointer',
            }}
            onClick={() => setExpandedKeys(prev =>
              prev.includes(node.key)
                ? prev.filter(k => k !== node.key)
                : [...prev, node.key]
            )}
          >
            <Space>
              {isExpanded ? <CaretDownOutlined /> : <CaretRightOutlined />}
              <FolderOutlined style={{ color: node.color }} />
              <Text strong style={{ color: node.color }}>{node.name}</Text>
              <Badge count={node.children?.length} style={{ backgroundColor: node.color }} />
            </Space>
          </div>
          {isExpanded && node.children && (
            <div style={{ marginLeft: 16 }}>
              {node.children.map(child => renderNode(child))}
            </div>
          )}
        </div>
      );
    }

    return (
      <div
        key={node.key}
        style={{
          padding: '8px 12px',
          marginLeft: node.depth * 24,
          marginBottom: 4,
          borderRadius: 6,
          cursor: 'pointer',
          transition: 'all 0.2s',
          border: '1px solid transparent',
        }}
        className="skill-item"
        onClick={() => node.data && onSkillClick(node.data, node.executor)}
        onMouseEnter={e => {
          e.currentTarget.style.background = `${node.color}10`;
          e.currentTarget.style.borderColor = node.color;
        }}
        onMouseLeave={e => {
          e.currentTarget.style.background = 'transparent';
          e.currentTarget.style.borderColor = 'transparent';
        }}
      >
        <Space>
          <FileTextOutlined style={{ color: node.color }} />
          <Text>{node.name}</Text>
          {node.data?.version && <Tag color={node.color} style={{ fontSize: 11 }}>v{node.data.version}</Tag>}
          <Text type="secondary" style={{ fontSize: 12 }}>{formatSize(node.data?.total_size || 0)}</Text>
        </Space>
      </div>
    );
  };

  return (
    <div>
      {data.map(executorData => {
        const nodes = buildTree(executorData);
        const executorLabel = EXECUTORS.find(e => e.value === executorData.executor)?.label || executorData.executor;

        return (
          <Card
            key={executorData.executor}
            size="small"
            title={
              <Space>
                <span style={{
                  width: 8,
                  height: 8,
                  borderRadius: '50%',
                  backgroundColor: executorData.skills_dir_exists
                    ? EXECUTOR_COLORS[executorData.executor] || '#7C3AED'
                    : '#d9d9d9',
                }} />
                <Text strong>{executorLabel}</Text>
                <Badge
                  count={executorData.skills.length}
                  style={{ backgroundColor: executorData.skills.length > 0 ? EXECUTOR_COLORS[executorData.executor] : '#d9d9d9' }}
                />
              </Space>
            }
            extra={
              <Space size="small">
                <Button
                  type="text"
                  size="small"
                  icon={<UploadOutlined />}
                  aria-label="导入 Skills"
                  onClick={() => onImport(executorData.executor)}
                />
                <Button
                  type="text"
                  size="small"
                  icon={<DownloadOutlined />}
                  aria-label="导出 Skills"
                  onClick={() => onExport(executorData.executor, false)}
                />
              </Space>
            }
            style={{ marginBottom: 12 }}
          >
            {!executorData.skills_dir_exists ? (
              <Empty description="目录不存在" image={Empty.PRESENTED_IMAGE_SIMPLE} />
            ) : nodes.length === 0 ? (
              <Empty description={searchText ? "无匹配结果" : "暂无 Skills"} image={Empty.PRESENTED_IMAGE_SIMPLE} />
            ) : (
              nodes.map(node => renderNode(node))
            )}
          </Card>
        );
      })}
    </div>
  );
}

// ── Skills 总览 ────────────────────────────────────────────

function SkillsOverview() {
  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<ExecutorSkills[]>([]);
  const [searchText, setSearchText] = useState('');
  const [showCategory, setShowCategory] = useState(true);
  const [selectedSkill, setSelectedSkill] = useState<SkillMeta | null>(null);
  const [selectedExecutor, setSelectedExecutor] = useState('');
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportMode, setExportMode] = useState<'import' | 'export'>('export');
  const [initialSelectedSkills, setInitialSelectedSkills] = useState<string[] | undefined>(undefined);

  useEffect(() => {
    setLoading(true);
    db.getSkillsList()
      .then(data => {
        setData(data);
        const withSkills = data.find(e => e.skills.length > 0);
        if (withSkills) {
          setSelectedExecutor(withSkills.executor);
        } else if (data.length > 0) {
          setSelectedExecutor(data[0].executor);
        }
      })
      .catch(err => message.error('加载失败: ' + err.message))
      .finally(() => setLoading(false));
  }, []);

  const handleSkillClick = (skill: SkillMeta, executor: string) => {
    setSelectedSkill(skill);
    setSelectedExecutor(executor);
    setDrawerOpen(true);
  };

  const totalSkills = useMemo(() => data.reduce((sum, e) => sum + e.skills.length, 0), [data]);
  const executorsWithSkills = useMemo(() => data.filter(e => e.skills.length > 0).length, [data]);

  const exportMenuItems: MenuProps['items'] = [
    { key: 'export', icon: <ExportOutlined />, label: '导出选中' },
    { key: 'export-all', icon: <ExportOutlined />, label: '导出全部' },
    { type: 'divider' },
    { key: 'import', icon: <ImportOutlined />, label: '导入' },
  ];

  const handleExportMenuClick: MenuProps['onClick'] = ({ key }) => {
    if (key === 'import') {
      setExportMode('import');
      setInitialSelectedSkills(undefined);
    } else {
      setExportMode('export');
      if (key === 'export-all') {
        const executorData = data.find(e => e.executor === selectedExecutor);
        if (executorData) {
          setInitialSelectedSkills(executorData.skills.map(s => s.name));
        }
      } else {
        setInitialSelectedSkills(undefined);
      }
    }
    setExportModalOpen(true);
  };

  const handleImport = (executor: string) => {
    setSelectedExecutor(executor);
    setExportMode('import');
    setInitialSelectedSkills(undefined);
    setExportModalOpen(true);
  };

  const handleExport = (executor: string, selectAll?: boolean) => {
    setSelectedExecutor(executor);
    setExportMode('export');
    if (selectAll) {
      const executorData = data.find(e => e.executor === executor);
      if (executorData) {
        setInitialSelectedSkills(executorData.skills.map(s => s.name));
      }
    } else {
      setInitialSelectedSkills(undefined);
    }
    setExportModalOpen(true);
  };

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 48 }}><Spin size="large" /></div>;
  }

  return (
    <div>
      {/* 统计卡片 */}
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col xs={24} sm={8}>
          <Card size="small">
            <Statistic
              title="Skill 总数"
              value={totalSkills}
              prefix={<ThunderboltOutlined style={{ color: '#7C3AED' }} />}
              valueStyle={{ color: '#7C3AED' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={8}>
          <Card size="small">
            <Statistic
              title="有 Skills 的执行器"
              value={executorsWithSkills}
              suffix={`/ ${data.length}`}
              prefix={<AppstoreOutlined style={{ color: '#10B981' }} />}
              valueStyle={{ color: '#10B981' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={8}>
          <Card size="small">
            <Statistic
              title="执行器总数"
              value={data.length}
              prefix={<BarChartOutlined style={{ color: '#F97316' }} />}
              valueStyle={{ color: '#F97316' }}
            />
          </Card>
        </Col>
      </Row>

      {/* 工具栏 */}
      <Card size="small" style={{ marginBottom: 16 }}>
        <Space wrap style={{ width: '100%', justifyContent: 'space-between' }}>
          <Space wrap>
            <Input
              placeholder="搜索 Skills..."
              prefix={<SearchOutlined />}
              value={searchText}
              onChange={e => setSearchText(e.target.value)}
              style={{ width: 200 }}
              allowClear
            />
            <Tooltip title={showCategory ? '显示扁平结构' : '显示目录结构'}>
              <Button
                icon={showCategory ? <FolderOpenOutlined /> : <FileOutlined />}
                onClick={() => setShowCategory(!showCategory)}
              >
                {showCategory ? '目录视图' : '扁平视图'}
              </Button>
            </Tooltip>
          </Space>
          <Dropdown menu={{ items: exportMenuItems, onClick: handleExportMenuClick }} trigger={['click']}>
            <Button type="primary" icon={<DownloadOutlined />}>
              导入/导出
            </Button>
          </Dropdown>
        </Space>
      </Card>

      {/* Skills 列表 */}
      <SkillTree
        data={data}
        onSkillClick={handleSkillClick}
        onImport={handleImport}
        onExport={handleExport}
        searchText={searchText}
        showCategory={showCategory}
      />

      {/* Skill 详情抽屉 */}
      <SkillDetailDrawer
        skill={selectedSkill}
        executor={selectedExecutor}
        executorLabel={EXECUTORS.find(e => e.value === selectedExecutor)?.label || selectedExecutor}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
      />

      {/* 导入导出 Modal */}
      <ImportExportModal
        open={exportModalOpen}
        mode={exportMode}
        executor={selectedExecutor}
        data={data}
        initialSelectedSkills={initialSelectedSkills}
        onClose={() => {
          setExportModalOpen(false);
          setInitialSelectedSkills(undefined);
        }}
      />
    </div>
  );
}

// ── 对比分析 ────────────────────────────────────────────────

function SkillsComparison() {
  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<SkillComparison[]>([]);
  const [filter, setFilter] = useState<'all' | 'shared' | 'unique'>('all');
  const [searchText, setSearchText] = useState('');

  useEffect(() => {
    setLoading(true);
    db.getSkillsComparison()
      .then(setData)
      .catch(err => message.error('加载失败: ' + err.message))
      .finally(() => setLoading(false));
  }, []);

  const filtered = useMemo(() => {
    let result = data;
    if (searchText) {
      const lower = searchText.toLowerCase();
      result = result.filter(s =>
        s.skill_name.toLowerCase().includes(lower) ||
        s.description?.toLowerCase().includes(lower)
      );
    }
    if (filter === 'shared') {
      result = result.filter(s => {
        const presentCount = Object.values(s.executors).filter(e => e.present).length;
        return presentCount >= 2;
      });
    } else if (filter === 'unique') {
      result = result.filter(s => {
        const presentCount = Object.values(s.executors).filter(e => e.present).length;
        return presentCount === 1;
      });
    }
    return result;
  }, [data, filter, searchText]);

  const executorColumns = EXECUTORS.map(exec => ({
    title: (
      <Tooltip title={exec.label}>
        <span style={{ fontSize: 12, color: exec.color }}>{exec.label}</span>
      </Tooltip>
    ),
    key: exec.value,
    width: 80,
    align: 'center' as const,
    render: (_: unknown, record: SkillComparison) => {
      const presence = record.executors[exec.value];
      if (!presence?.present) {
        return <span style={{ color: '#d9d9d9' }}>-</span>;
      }
      return (
        <Tooltip title={presence.version ? `v${presence.version}` : '已安装'}>
          <CheckCircleOutlined style={{ color: exec.color, fontSize: 16 }} />
        </Tooltip>
      );
    },
  }));

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 48 }}><Spin size="large" /></div>;
  }

  const sharedCount = data.filter(s => Object.values(s.executors).filter(e => e.present).length >= 2).length;
  const uniqueCount = data.filter(s => Object.values(s.executors).filter(e => e.present).length === 1).length;

  return (
    <div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Input.Search
          placeholder="搜索 Skill"
          value={searchText}
          onChange={e => setSearchText(e.target.value)}
          style={{ width: 200 }}
          allowClear
          prefix={<SearchOutlined />}
        />
        <Select value={filter} onChange={setFilter} style={{ width: 140 }}>
          <Select.Option value="all">全部 ({data.length})</Select.Option>
          <Select.Option value="shared">共享 ({sharedCount})</Select.Option>
          <Select.Option value="unique">独有 ({uniqueCount})</Select.Option>
        </Select>
      </Space>

      {filtered.length === 0 ? (
        <Empty description="没有匹配的 Skills" />
      ) : (
        <Table
          dataSource={filtered}
          rowKey="skill_name"
          size="small"
          pagination={{ pageSize: 20 }}
          scroll={{ x: 900 }}
          columns={[
            {
              title: 'Skill',
              dataIndex: 'skill_name',
              width: 180,
              fixed: 'left',
              render: (name: string, record: SkillComparison) => {
                const presentCount = Object.values(record.executors).filter(e => e.present).length;
                const totalExecs = EXECUTORS.length;
                let tagColor = 'default';
                let tagLabel = '';
                if (presentCount >= 3) { tagColor = 'green'; tagLabel = '热门'; }
                else if (presentCount >= 2) { tagColor = 'blue'; tagLabel = '共享'; }
                else { tagColor = 'orange'; tagLabel = '独有'; }
                return (
                  <div>
                    <Text strong>{name}</Text>
                    <Tag color={tagColor} style={{ marginLeft: 4, fontSize: 10 }}>{tagLabel}</Tag>
                    <div style={{ marginTop: 2 }}>
                      <Text type="secondary" style={{ fontSize: 11 }}>
                        {presentCount}/{totalExecs} 执行器
                      </Text>
                    </div>
                  </div>
                );
              },
            },
            {
              title: '描述',
              dataIndex: 'description',
              width: 200,
              ellipsis: true,
              render: (desc: string) => (
                <Tooltip title={desc}>
                  <Text type="secondary" ellipsis style={{ fontSize: 12 }}>{desc || '-'}</Text>
                </Tooltip>
              ),
            },
            ...executorColumns,
          ]}
        />
      )}
    </div>
  );
}

// ── Skill 同步 ──────────────────────────────────────────────

function SkillSync() {
  const [loading, setLoading] = useState(true);
  const [executors, setExecutors] = useState<ExecutorSkills[]>([]);
  const [selectedExecutor, setSelectedExecutor] = useState<string | null>(null);
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null);
  const [targetExecutors, setTargetExecutors] = useState<string[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [syncResult, setSyncResult] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    db.getSkillsList()
      .then(data => {
        setExecutors(data.filter(e => e.skills_dir_exists));
      })
      .catch(err => message.error('加载失败: ' + err.message))
      .finally(() => setLoading(false));
  }, []);

  const sourceSkills = useMemo(() => {
    if (!selectedExecutor) return [];
    return executors.find(e => e.executor === selectedExecutor)?.skills || [];
  }, [selectedExecutor, executors]);

  const handleSync = async () => {
    if (!selectedExecutor || !selectedSkill || targetExecutors.length === 0) {
      message.warning('请选择源执行器、Skill 和目标执行器');
      return;
    }
    setSyncing(true);
    setSyncResult(null);
    try {
      const result = await db.syncSkill(selectedExecutor, selectedSkill, targetExecutors);
      setSyncResult(result);
      message.success('同步完成');
    } catch (err: any) {
      message.error('同步失败: ' + (err?.message || String(err)));
    } finally {
      setSyncing(false);
    }
  };

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 48 }}><Spin size="large" /></div>;
  }

  return (
    <div style={{ maxWidth: 800 }}>
      <Card title="Skill 同步" size="small" style={{ marginBottom: 16 }}>
        <Paragraph type="secondary" style={{ marginBottom: 16 }}>
          将一个执行器下的 Skill 复制到其他执行器。支持批量同步到多个目标。
        </Paragraph>

        <Space direction="vertical" style={{ width: '100%' }} size="middle">
          <div>
            <Text strong style={{ display: 'block', marginBottom: 8 }}>1. 选择源执行器</Text>
            <Select
              value={selectedExecutor}
              onChange={v => { setSelectedExecutor(v); setSelectedSkill(null); }}
              style={{ width: '100%' }}
              placeholder="选择有 Skills 的执行器"
            >
              {executors.map(e => (
                <Select.Option key={e.executor} value={e.executor}>
                  <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span style={{
                      width: 8, height: 8, borderRadius: '50%',
                      backgroundColor: EXECUTOR_COLORS[e.executor] || '#7C3AED',
                    }} />
                    {e.executor_label}
                    <Tag>{e.skills.length} Skills</Tag>
                  </span>
                </Select.Option>
              ))}
            </Select>
          </div>

          {selectedExecutor && (
            <div>
              <Text strong style={{ display: 'block', marginBottom: 8 }}>2. 选择要同步的 Skill</Text>
              <Select
                value={selectedSkill}
                onChange={setSelectedSkill}
                style={{ width: '100%' }}
                placeholder="选择 Skill"
                showSearch
                optionFilterProp="label"
              >
                {sourceSkills.map(s => (
                  <Select.Option key={s.name} value={s.name} label={s.name}>
                    <span>
                      <Text strong>{s.name}</Text>
                      {s.version && <Tag color="blue" style={{ marginLeft: 8 }}>v{s.version}</Tag>}
                      <Text type="secondary" style={{ marginLeft: 8, fontSize: 11 }}>{formatSize(s.total_size)}</Text>
                    </span>
                  </Select.Option>
                ))}
              </Select>
            </div>
          )}

          {selectedSkill && (
            <div>
              <Text strong style={{ display: 'block', marginBottom: 8 }}>3. 选择目标执行器</Text>
              <Checkbox.Group
                value={targetExecutors}
                onChange={v => setTargetExecutors(v as string[])}
                style={{ width: '100%' }}
              >
                <Row gutter={[8, 8]}>
                  {EXECUTORS.filter(e => e.value !== selectedExecutor).map(exec => {
                    const exists = executors.find(ex => ex.executor === exec.value);
                    const alreadyHas = exists?.skills.find(s => s.name === selectedSkill);
                    return (
                      <Col span={12} key={exec.value}>
                        <Checkbox value={exec.value}>
                          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                            <span style={{
                              width: 6, height: 6, borderRadius: '50%',
                              backgroundColor: exec.color,
                            }} />
                            {exec.label}
                            {alreadyHas && <Tag color="orange" style={{ fontSize: 10 }}>已存在</Tag>}
                          </span>
                        </Checkbox>
                      </Col>
                    );
                  })}
                </Row>
              </Checkbox.Group>
            </div>
          )}

          {syncResult && (
            <Alert
              message={syncResult}
              type="success"
              showIcon
            />
          )}

          <div style={{ textAlign: 'right' }}>
            <Button
              type="primary"
              icon={<CopyOutlined />}
              onClick={handleSync}
              loading={syncing}
              disabled={!selectedSkill || targetExecutors.length === 0}
            >
              同步到 {targetExecutors.length} 个执行器
            </Button>
          </div>
        </Space>
      </Card>
    </div>
  );
}

// ── Skill 追踪 ──────────────────────────────────────────────

function SkillTracking() {
  const [loading, setLoading] = useState(true);
  const [invocations, setInvocations] = useState<SkillInvocation[]>([]);
  const [page, setPage] = useState(1);
  const [totalCount, setTotalCount] = useState(0);
  const [filterSkill, setFilterSkill] = useState<string | undefined>();
  const [filterExecutor, setFilterExecutor] = useState<string | undefined>();

  const loadData = async (p: number, skill?: string, executor?: string) => {
    setLoading(true);
    try {
      const data = await db.getSkillInvocations({
        page: p,
        limit: 20,
        skill_name: skill,
        executor,
      });
      setInvocations(data.items);
      setTotalCount(data.total);
    } catch (err: any) {
      message.error('加载失败: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { loadData(1); }, []);

  const handleRefresh = () => loadData(page, filterSkill, filterExecutor);

  const skillStats = useMemo(() => {
    const map = new Map<string, { count: number; executors: Set<string> }>();
    invocations.forEach(inv => {
      const s = map.get(inv.skill_name) || { count: 0, executors: new Set<string>() };
      s.count++;
      s.executors.add(inv.executor);
      map.set(inv.skill_name, s);
    });
    return Array.from(map.entries())
      .map(([name, data]) => ({ name, count: data.count, executorCount: data.executors.size }))
      .sort((a, b) => b.count - a.count);
  }, [invocations]);

  const isMobile = useIsMobile(640);

  return (
    <div>
      {skillStats.length > 0 && (
        <Row gutter={16} style={{ marginBottom: 16 }}>
          {skillStats.slice(0, isMobile ? 3 : 4).map(stat => (
            <Col xs={24} sm={12} md={6} key={stat.name}>
              <Card size="small">
                <Statistic
                  title={<Text ellipsis style={{ maxWidth: isMobile ? 80 : 120, fontSize: 12 }}>{stat.name}</Text>}
                  value={stat.count}
                  suffix="次"
                  valueStyle={{ fontSize: 18 }}
                />
                <Text type="secondary" style={{ fontSize: 10 }}>{stat.executorCount} 个执行器</Text>
              </Card>
            </Col>
          ))}
        </Row>
      )}

      <Card size="small" style={{ marginBottom: 16 }}>
        <Space wrap>
          <Input.Search
            placeholder="按 Skill 名称筛选"
            allowClear
            style={{ width: isMobile ? '100%' : 200 }}
            onSearch={v => { setFilterSkill(v || undefined); setPage(1); loadData(1, v || undefined, filterExecutor); }}
            prefix={<SearchOutlined />}
          />
          <Select
            placeholder="按执行器筛选"
            allowClear
            style={{ width: isMobile ? '100%' : 150 }}
            onChange={v => { setFilterExecutor(v || undefined); setPage(1); loadData(1, filterSkill, v || undefined); }}
          >
            {EXECUTORS.map(e => (
              <Select.Option key={e.value} value={e.value}>{e.label}</Select.Option>
            ))}
          </Select>
          <Button icon={<ReloadOutlined />} onClick={handleRefresh}>刷新</Button>
        </Space>
      </Card>

      {invocations.length === 0 ? (
        <Empty description="暂无调用记录" />
      ) : (
        <Table
          dataSource={invocations}
          rowKey="id"
          size="small"
          loading={loading}
          pagination={{
            current: page,
            pageSize: 20,
            total: totalCount,
            onChange: p => { setPage(p); loadData(p, filterSkill, filterExecutor); },
          }}
          columns={[
            {
              title: 'Skill',
              dataIndex: 'skill_name',
              width: 180,
              render: (name: string) => (
                <Text strong style={{ color: '#7C3AED' }}>{name}</Text>
              ),
            },
            {
              title: '执行器',
              dataIndex: 'executor',
              width: 120,
              render: (exec: string) => {
                const opt = EXECUTORS.find(e => e.value === exec.toLowerCase());
                return (
                  <Tag color={opt?.color || 'default'}>
                    {opt?.label || exec}
                  </Tag>
                );
              },
            },
            {
              title: '关联 Todo',
              dataIndex: 'todo_title',
              width: 200,
              ellipsis: true,
              render: (title: string | null, record: SkillInvocation) => (
                <Tooltip title={title || `Todo #${record.todo_id}`}>
                  <Text type="secondary" ellipsis>{title || `Todo #${record.todo_id}`}</Text>
                </Tooltip>
              ),
            },
            {
              title: '状态',
              dataIndex: 'status',
              width: 100,
              render: (status: string) => {
                const map: Record<string, { color: string; label: string }> = {
                  invoked: { color: 'processing', label: '已调用' },
                  completed: { color: 'success', label: '完成' },
                  failed: { color: 'error', label: '失败' },
                };
                const s = map[status] || { color: 'default', label: status };
                return <Tag color={s.color}>{s.label}</Tag>;
              },
            },
            {
              title: '耗时',
              dataIndex: 'duration_ms',
              width: 100,
              render: (ms: number | null) => ms != null ? `${(ms / 1000).toFixed(1)}s` : '-',
            },
            {
              title: '调用时间',
              dataIndex: 'invoked_at',
              width: 150,
              render: (t: string) => <Text type="secondary" style={{ fontSize: 12 }}>{formatTime(t)}</Text>,
            },
          ]}
        />
      )}
    </div>
  );
}

// ── 主 Skills Panel ────────────────────────────────────────

type SubView = 'overview' | 'compare' | 'sync' | 'tracking';

export function SkillsPanel() {
  const [activeView, setActiveView] = useState<SubView>('overview');

  const views: { key: SubView; label: string; icon: ReactNode }[] = [
    { key: 'overview', label: 'Skills 总览', icon: <AppstoreOutlined /> },
    { key: 'compare', label: '对比分析', icon: <BarChartOutlined /> },
    { key: 'sync', label: '同步管理', icon: <SwapOutlined /> },
    { key: 'tracking', label: '调用追踪', icon: <ThunderboltOutlined /> },
  ];

  return (
    <div>
      <div style={{
        display: 'flex',
        flexWrap: 'wrap',
        gap: 8,
        marginBottom: 20,
        borderBottom: '1px solid var(--color-border-light, #f0f0f0)',
        paddingBottom: 12,
      }}>
        {views.map(v => (
          <Button
            key={v.key}
            type={activeView === v.key ? 'primary' : 'default'}
            icon={v.icon}
            onClick={() => setActiveView(v.key)}
            style={{ borderRadius: 8, fontSize: 13, padding: '4px 10px' }}
          >
            {v.label}
          </Button>
        ))}
      </div>

      {activeView === 'overview' && <SkillsOverview />}
      {activeView === 'compare' && <SkillsComparison />}
      {activeView === 'sync' && <SkillSync />}
      {activeView === 'tracking' && <SkillTracking />}
    </div>
  );
}
