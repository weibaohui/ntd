import { Card, Button, Typography, Upload, Select, Space, Modal, Tag, Alert, message, Radio, Table, Divider } from 'antd';
import { DownloadOutlined, InboxOutlined } from '@ant-design/icons';
import { useState, useEffect } from 'react';
import * as db from '@/utils/database';
import { exportLoop, listLoops } from '@/utils/database/loops';
import { LoopImportPreview } from '@/utils/database/backup';

const { Dragger } = Upload;
const { Group: RadioGroup, Button: RadioButton } = Radio;

type ImportMode = 'create' | 'merge';
type ConflictAction = 'rename' | 'overwrite' | 'skip';

export function LoopBackupTab() {
  const [selectedLoopId, setSelectedLoopId] = useState<number | null>(null);
  const [exporting, setExporting] = useState(false);
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [yamlPreview, setYamlPreview] = useState<string | null>(null);
  const [previewData, setPreviewData] = useState<LoopImportPreview | null>(null);
  const [importing, setImporting] = useState(false);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<number | null>(null);
  const [workspaces, setWorkspaces] = useState<any[]>([]);
  const [loops, setLoops] = useState<any[]>([]);
  // 导入模式：create=新建模式（默认），merge=合并模式
  const [importMode, setImportMode] = useState<ImportMode>('create');
  // 冲突解决映射：loop_name -> action
  const [conflictResolutions, setConflictResolutions] = useState<Record<string, ConflictAction>>({});

  // 加载环路列表
  useEffect(() => {
    listLoops().then(setLoops).catch(() => {});
  }, []);

  // 加载工作空间列表
  const loadWorkspaces = async () => {
    try {
      const ws = await db.getProjectDirectories();
      setWorkspaces(ws);
      if (ws.length > 0 && !selectedWorkspaceId) {
        setSelectedWorkspaceId(ws[0].id);
      }
    } catch (e) {
      console.error('Failed to load workspaces', e);
    }
  };

  // 导出单个环路
  const handleExportLoop = async (loopId: number) => {
    setExporting(true);
    try {
      const yaml = await exportLoop(loopId);
      const blob = new Blob([yaml], { type: 'application/x-yaml' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const loop = loops.find((l: any) => l.id === loopId);
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      a.download = `${loop?.name || 'loop'}-${timestamp}.loop.yaml`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success('环路导出成功');
    } catch (err: any) {
      message.error(err?.message || '导出失败');
    } finally {
      setExporting(false);
    }
  };

  // 导入文件解析
  const handleImportFile = async (file: File) => {
    try {
      const text = await file.text();
      const preview = await db.previewLoopImport(text);
      setYamlPreview(text);
      setPreviewData(preview);
      // 初始化冲突解决策略：默认重命名
      const resolutions: Record<string, ConflictAction> = {};
      if (preview.conflicts) {
        for (const c of preview.conflicts) {
          resolutions[c.name] = 'rename';
        }
      }
      setConflictResolutions(resolutions);
      await loadWorkspaces();
      setImportModalOpen(true);
    } catch (err: any) {
      message.error('解析文件失败: ' + (err?.message || String(err)));
    }
    return false;
  };

  // 更新单个冲突的解决策略
  const updateConflictResolution = (name: string, action: ConflictAction) => {
    setConflictResolutions(prev => ({ ...prev, [name]: action }));
  };

  // 执行导入
  const handleConfirmImport = async () => {
    if (!yamlPreview || !selectedWorkspaceId) {
      message.warning('请选择目标工作空间');
      return;
    }
    setImporting(true);
    try {
      if (importMode === 'merge') {
        const result = await db.mergeLoops(yamlPreview, selectedWorkspaceId, conflictResolutions);
        message.success(
          `导入完成：新建 ${result.created.loops} 个，更新 ${result.updated?.loops || 0} 个，跳过 ${result.skipped?.length || 0} 个`
        );
      } else {
        const result = await db.importLoops(yamlPreview, selectedWorkspaceId);
        message.success(`导入成功：创建了 ${result.created.loops} 个环路`);
      }
      setImportModalOpen(false);
      setYamlPreview(null);
      setPreviewData(null);
      window.location.reload();
    } catch (err: any) {
      message.error(err?.message || '导入失败');
    } finally {
      setImporting(false);
    }
  };

  const loopOptions = loops.map((l: any) => ({ label: l.name, value: l.id }));

  const conflictColumns = [
    { title: '环路名称', dataIndex: 'name', key: 'name' },
    {
      title: '解决策略',
      dataIndex: 'action',
      render: (_: any, record: any) => (
        <RadioGroup
          value={conflictResolutions[record.name] || 'rename'}
          onChange={e => updateConflictResolution(record.name, e.target.value)}
          size="small"
        >
          <RadioButton value="rename">重命名</RadioButton>
          <RadioButton value="overwrite">覆盖</RadioButton>
          <RadioButton value="skip">跳过</RadioButton>
        </RadioGroup>
      ),
    },
  ];

  return (
    <div style={{ maxWidth: 600 }}>
      <Card title="导出环路" size="small" style={{ marginBottom: 24 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <Typography.Paragraph type="secondary">
            将环路导出为 .loop.yaml 文件，方便迁移和分享
          </Typography.Paragraph>
          <Select
            placeholder="选择一个环路"
            options={loopOptions}
            value={selectedLoopId}
            onChange={setSelectedLoopId}
            style={{ width: '100%' }}
            allowClear
          />
          <Button
            type="primary"
            icon={<DownloadOutlined />}
            onClick={() => selectedLoopId && handleExportLoop(selectedLoopId)}
            loading={exporting}
            disabled={!selectedLoopId}
            style={{ width: '100%' }}
          >
            导出选中环路
          </Button>
        </div>
      </Card>

      <Card title="导入环路" size="small" style={{ marginBottom: 24 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <Typography.Paragraph type="secondary">
            从 .loop.yaml 文件导入环路，先选择目标工作空间，再上传文件
          </Typography.Paragraph>
          {/* 目标工作空间选择前置到导入区域——用户必须选择后才能上传 */}
          <div>
            <Typography.Text strong style={{ fontSize: 13 }}>目标工作空间</Typography.Text>
            <Select
              placeholder="请选择工作空间"
              options={workspaces.map((w: any) => ({ label: w.name || w.path, value: w.id }))}
              value={selectedWorkspaceId}
              onChange={setSelectedWorkspaceId}
              style={{ width: '100%', marginTop: 4 }}
            />
            <Typography.Paragraph type="secondary" style={{ margin: '4px 0 0', fontSize: 11 }}>
              选择导入后环路和 Todo 所属的工作空间
            </Typography.Paragraph>
          </div>
          <Dragger
            accept=".yaml,.yml,.loop.yaml"
            beforeUpload={handleImportFile}
            showUploadList={false}
            style={{ borderRadius: 12 }}
          >
            <p className="ant-upload-drag-icon">
              <InboxOutlined style={{ color: '#0891b2' }} />
            </p>
            <p className="ant-upload-text">点击或拖拽 .loop.yaml 文件到此处</p>
            <p className="ant-upload-hint">将解析文件并展示预览，确认后导入到选中的工作空间</p>
          </Dragger>
        </div>
      </Card>

      <Modal
        title="导入环路预览"
        open={importModalOpen}
        onCancel={() => setImportModalOpen(false)}
        footer={[
          <Button key="cancel" onClick={() => setImportModalOpen(false)}>取消</Button>,
          <Button
            key="import"
            type="primary"
            loading={importing}
            disabled={!selectedWorkspaceId}
            onClick={handleConfirmImport}
          >
            确认导入
          </Button>,
        ]}
        width={700}
      >
        {previewData && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            <Alert
              message="即将导入以下内容"
              description={
                <Space direction="vertical" size="small">
                  <Tag>环路: {previewData.summary.loops} 个</Tag>
                  <Tag>步骤: {previewData.summary.steps} 个</Tag>
                  <Tag>Todo模板: {previewData.summary.todos} 个</Tag>
                  <Tag>评审模板: {previewData.summary.review_templates} 个</Tag>
                  <Tag>标签: {previewData.summary.tags} 个</Tag>
                  <Tag>触发器: {previewData.summary.triggers} 个</Tag>
                </Space>
              }
              type="info"
              showIcon
            />

            {previewData.warnings && previewData.warnings.length > 0 && (
              <Alert
                message="警告"
                description={
                  <ul style={{ margin: 0, paddingLeft: 20 }}>
                    {previewData.warnings.map((w: any, i: number) => (
                      <li key={i}>{w.message}</li>
                    ))}
                  </ul>
                }
                type="warning"
                showIcon
              />
            )}

            <div>
              <Typography.Text strong>目标工作空间</Typography.Text>
              <Select
                placeholder="选择工作空间"
                options={workspaces.map((w: any) => ({ label: w.name, value: w.id }))}
                value={selectedWorkspaceId}
                onChange={setSelectedWorkspaceId}
                style={{ width: '100%', marginTop: 8 }}
              />
            </div>

            <Divider style={{ margin: '12px 0' }} />

            <div>
              <Typography.Text strong>导入模式</Typography.Text>
              <RadioGroup
                value={importMode}
                onChange={e => setImportMode(e.target.value)}
                style={{ marginLeft: 12 }}
              >
                <RadioButton value="create">新建模式</RadioButton>
                <RadioButton value="merge" disabled={!previewData.conflicts?.length}>
                  合并模式
                </RadioButton>
              </RadioGroup>
              <Typography.Paragraph type="secondary" style={{ marginTop: 4, marginBottom: 0 }}>
                {importMode === 'create'
                  ? '所有环路作为全新实体创建，同名自动追加 "-导入" 后缀'
                  : '同名环路按下方策略处理'}
              </Typography.Paragraph>
            </div>

            {importMode === 'merge' && previewData.conflicts && previewData.conflicts.length > 0 && (
              <>
                <Divider style={{ margin: '12px 0' }} />
                <div>
                  <Typography.Text strong>冲突解决策略</Typography.Text>
                  <Typography.Paragraph type="secondary">
                    检测到 {previewData.conflicts.length} 个同名环路，请选择处理方式
                  </Typography.Paragraph>
                  <Table
                    size="small"
                    dataSource={previewData.conflicts.map((c: any) => ({ key: c.name, ...c }))}
                    columns={conflictColumns}
                    pagination={false}
                    style={{ marginTop: 8 }}
                  />
                </div>
              </>
            )}
          </div>
        )}
      </Modal>
    </div>
  );
}
