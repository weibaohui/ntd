import { Card, Button, Typography, Upload, Space, Modal, Tag, Alert, message, Table, Divider, Select } from 'antd';
import { DownloadOutlined, InboxOutlined } from '@ant-design/icons';
import { useState } from 'react';
import * as db from '@/utils/database';
import { listLoops, exportAllLoops } from '@/utils/database/loops';
import { LoopImportPreview } from '@/utils/database/backup';
import type { ProjectDirectory } from '@/utils/database/todos';
import { WorkspaceSwitcher } from '@/components/shell/WorkspaceSwitcher';

const { Dragger } = Upload;

export function LoopBackupTab() {
  const [exporting, setExporting] = useState(false);
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [yamlPreview, setYamlPreview] = useState<string | null>(null);
  const [previewData, setPreviewData] = useState<LoopImportPreview | null>(null);
  const [importing, setImporting] = useState(false);
  // 工作空间列表（父组件加载一次，下发给每行 WorkspaceSwitcher 复用）
  const [workspaces, setWorkspaces] = useState<ProjectDirectory[]>([]);
  // 逐 loop 工作空间选择：loop name → workspaceId（null=未匹配/待指定）
  const [loopWorkspaceMap, setLoopWorkspaceMap] = useState<Record<string, number | null>>({});
  // 当前库已有 loop 名集合，供「同名处理」列判断新建/同名（对齐 Todo 用已有 todo 判定 action）
  const [existingLoopNames, setExistingLoopNames] = useState<Set<string>>(new Set());
  // 同名 loop 的导入动作：loop name → 'overwrite' | 'skip'（默认 skip，避免误覆盖）；新建 loop 不入此表
  const [loopActionMap, setLoopActionMap] = useState<Record<string, 'overwrite' | 'skip'>>({});

  // 设置某条 loop 的目标工作空间（供 per-loop 表回写）
  const setLoopWorkspace = (name: string, id: number | null) => {
    setLoopWorkspaceMap((prev) => ({ ...prev, [name]: id }));
  };

  // 设置某条同名 loop 的导入动作（覆盖/跳过）；setBulkLoopAction 批量作用于全部同名 loop
  const setLoopAction = (name: string, action: 'overwrite' | 'skip') => {
    setLoopActionMap((prev) => ({ ...prev, [name]: action }));
  };
  const setBulkLoopAction = (action: 'overwrite' | 'skip') => {
    setLoopActionMap((prev) => {
      const next = { ...prev };
      for (const l of previewData?.loops ?? []) {
        if (existingLoopNames.has(l.name)) next[l.name] = action;
      }
      return next;
    });
  };

  // 导出全库所有环路为单个 YAML（对齐 Todo「导出全部」）
  // v1 workspace-scoped：逐空间导出后拼接
  const handleExportAll = async () => {
    setExporting(true);
    try {
      // 先确保工作空间列表已加载
      const ws = workspaces.length > 0 ? workspaces : await db.getProjectDirectories();
      if (workspaces.length === 0) setWorkspaces(ws);
      // 逐空间导出后拼接（v1 不支持跨空间批量端点）
      const yamlTexts = await Promise.all(
        ws.map(w => exportAllLoops(w.id).catch(() => ''))
      );
      const yamlText = yamlTexts.filter(Boolean).join('\n---\n');
      const blob = new Blob([yamlText], { type: 'application/x-yaml' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      a.download = `loops-export-${timestamp}.loop.yaml`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success('环路导出成功');
    } catch (err) {
      message.error(err instanceof Error ? err.message : '导出失败');
    } finally {
      setExporting(false);
    }
  };

  // 导入文件解析
  const handleImportFile = async (file: File) => {
    try {
      const text = await file.text();
      // v1 workspace-scoped：先加载工作空间列表，取第一个作为 URL 上下文空间
      const ws = workspaces.length > 0 ? workspaces : await db.getProjectDirectories();
      if (workspaces.length === 0) setWorkspaces(ws);
      const ctxWs = ws[0]?.id ?? 0;
      const preview = await db.previewLoopImport(ctxWs, text);
      setYamlPreview(text);
      setPreviewData(preview);

      // 并行加载当前库已有 loop 名（逐空间拉取后合并，用于「状态」列同名检查）
      const allLoops = await Promise.all(
        ws.map(w => listLoops(w.id).catch(() => []))
      );
      const loops = allLoops.flat();
      setWorkspaces(ws);
      setExistingLoopNames(new Set(loops.map((l) => l.name)));

      // 按预览里的 per-loop 匹配情况初始化默认归属：
      // 原工作空间命中→原 id，否则 null（未匹配，需用户逐条指定）
      const wsMap: Record<string, number | null> = {};
      for (const l of preview.loops) {
        wsMap[l.name] = l.source_matched ? l.resolved_workspace_id : null;
      }
      setLoopWorkspaceMap(wsMap);
      // 同名 loop 默认「跳过」（保守，避免误覆盖）；用户可逐条或批量改为「覆盖」
      const actionMap: Record<string, 'overwrite' | 'skip'> = {};
      const existing = new Set(loops.map((l) => l.name));
      for (const l of preview.loops) {
        if (existing.has(l.name)) actionMap[l.name] = 'skip';
      }
      setLoopActionMap(actionMap);
      setImportModalOpen(true);
    } catch (err) {
      message.error('解析文件失败: ' + (err instanceof Error ? err.message : String(err)));
    }
    return false;
  };

  // 执行导入——同名按用户动作（覆盖/跳过，默认跳过），否则新建；对齐 Todo
  const handleConfirmImport = async () => {
    if (!yamlPreview || !previewData) {
      return;
    }
    // 收集「跳过」的同名 loop 名（默认 skip；用户改为 overwrite 的不计入）
    const skipNames = previewData.loops
      .filter((l) => existingLoopNames.has(l.name) && (loopActionMap[l.name] ?? 'skip') === 'skip')
      .map((l) => l.name);
    const skipSet = new Set(skipNames);
    // gate：仅对「非跳过」的 loop 要求指定工作空间（跳过的不导入，无需归属）
    const unassigned = previewData.loops.filter(
      (l) => !skipSet.has(l.name) && loopWorkspaceMap[l.name] == null,
    );
    if (unassigned.length > 0) {
      message.warning(`以下环路未指定工作空间: ${unassigned.map((l) => l.name).join(', ')}`);
      return;
    }
    // 构造 per-loop overrides（仅含非跳过且已指定的项），全局 workspace_id 传 null
    const overrides: Record<string, number> = {};
    for (const [name, id] of Object.entries(loopWorkspaceMap)) {
      if (id != null && !skipSet.has(name)) overrides[name] = id;
    }
    setImporting(true);
    try {
      // v1 workspace-scoped：用第一个工作空间作为 URL 上下文，逐 loop 的目标由 overrides 指定
      const ctxWs = workspaces[0]?.id ?? 0;
      const result = await db.mergeLoops(ctxWs, yamlPreview, null, overrides, skipNames);
      message.success(
        `导入完成：新建 ${result.created.loops} 个，更新 ${result.updated?.loops || 0} 个，跳过 ${result.skipped?.length || 0} 个`
      );
      setImportModalOpen(false);
      setYamlPreview(null);
      setPreviewData(null);
      window.location.reload();
    } catch (err) {
      message.error(err instanceof Error ? err.message : '导入失败');
    } finally {
      setImporting(false);
    }
  };

  // per-loop 工作空间表列：环路 | 状态(新建/覆盖) | 工作空间 | 来源
  const loopWsColumns = [
    { title: '环路', dataIndex: 'name', key: 'name', width: 120 },
    {
      title: '同名处理',
      key: 'status',
      width: 104,
      // 同名 loop 可在「覆盖/跳过」间切换（默认跳过）；纯新建 loop 固定显示「新建」
      render: (_: unknown, r: { name: string }) =>
        existingLoopNames.has(r.name) ? (
          <Select
            size="small"
            value={loopActionMap[r.name] ?? 'skip'}
            onChange={(v) => setLoopAction(r.name, v)}
            style={{ width: 96 }}
            options={[
              { value: 'overwrite', label: '覆盖' },
              { value: 'skip', label: '跳过' },
            ]}
          />
        ) : (
          <Tag color="green">新建</Tag>
        ),
    },
    {
      title: '工作空间',
      key: 'ws',
      width: 200,
      render: (_: unknown, r: { name: string }) => (
        <WorkspaceSwitcher
          dirs={workspaces}
          value={loopWorkspaceMap[r.name] ?? null}
          showAddOption={false}
          onChange={(id) => setLoopWorkspace(r.name, id)}
        />
      ),
    },
    {
      title: '来源',
      key: 'source',
      width: 80,
      render: (_: unknown, r: { source_matched: boolean }) =>
        r.source_matched ? <Tag color="blue">原始</Tag> : <Tag color="red">未匹配</Tag>,
    },
  ];

  // 派生：同名 loop / 各动作计数 / 将跳过集合——批量按钮与 gate 复用
  const loops = previewData?.loops ?? [];
  const sameNameLoops = loops.filter((l) => existingLoopNames.has(l.name));
  const sameNameLoopNames = sameNameLoops.map((l) => l.name);
  const isOverwrite = (name: string) => existingLoopNames.has(name) && loopActionMap[name] === 'overwrite';
  const newCount = loops.length - sameNameLoops.length;
  const overwriteCount = sameNameLoops.filter((l) => isOverwrite(l.name)).length;
  const skipCount = sameNameLoops.length - overwriteCount;
  const skipNameSet = new Set(sameNameLoops.filter((l) => !isOverwrite(l.name)).map((l) => l.name));
  // OK gate：任一「非跳过」loop 未指定工作空间则禁用
  const hasUnassignedLoop = loops.some((l) => !skipNameSet.has(l.name) && loopWorkspaceMap[l.name] == null);

  return (
    <div style={{ width: '100%' }}>
      <Card title="导出环路" size="small" style={{ marginBottom: 24 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <Typography.Paragraph type="secondary">
            将全库所有环路导出为 .loop.yaml 文件，方便迁移和分享
          </Typography.Paragraph>
          <Button
            type="primary"
            icon={<DownloadOutlined />}
            onClick={handleExportAll}
            loading={exporting}
            style={{ width: '100%' }}
          >
            导出全部环路
          </Button>
        </div>
      </Card>

      <Card title="导入环路" size="small" style={{ marginBottom: 24 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <Typography.Paragraph type="secondary">
            从 .loop.yaml 文件导入环路；解析后可逐条 loop 指定目标工作空间
          </Typography.Paragraph>
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
            <p className="ant-upload-hint">将解析文件并展示预览，逐条指派工作空间后导入</p>
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
            disabled={hasUnassignedLoop}
            onClick={handleConfirmImport}
          >
            确认导入
          </Button>,
        ]}
        width={780}
      >
        {previewData && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            {/* 同名处理统计 + 批量动作 */}
            <div style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
              <Tag color="green">{newCount} 新建</Tag>
              <Tag color="orange">{overwriteCount} 覆盖</Tag>
              <Tag>{skipCount} 跳过</Tag>
              <Button size="small" disabled={sameNameLoopNames.length === 0} onClick={() => setBulkLoopAction('overwrite')}>同名全覆盖</Button>
              <Button size="small" disabled={sameNameLoopNames.length === 0} onClick={() => setBulkLoopAction('skip')}>同名全跳过</Button>
            </div>
            {/* per-loop 工作空间指派 + 同名处理（默认跳过）：默认按原 id 匹配，可逐条点改 */}
            <div>
              <Typography.Text strong>环路工作空间（逐条）</Typography.Text>
              <Table
                size="small"
                pagination={false}
                rowKey="name"
                dataSource={previewData.loops}
                columns={loopWsColumns}
                style={{ marginTop: 4 }}
              />
            </div>

            <Divider style={{ margin: '4px 0' }} />

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
                    {previewData.warnings.map((w, i) => (
                      <li key={i}>{w.message}</li>
                    ))}
                  </ul>
                }
                type="warning"
                showIcon
              />
            )}
          </div>
        )}
      </Modal>
    </div>
  );
}
