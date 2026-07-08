import { Modal, Table, Tag as AntTag, Divider, Typography, Alert } from 'antd';
import { InfoCircleOutlined } from '@ant-design/icons';

export interface BackupDataYaml {
  version: string;
  created_at: string;
  tags: { name: string; color: string }[];
  todos: {
    title: string;
    prompt: string;
    status: string;
    executor?: string;
    scheduler_enabled: boolean;
    scheduler_config?: string;
    tag_names: string[];
    workspace_path?: string;
  }[];
}

export interface ImportItem {
  key: number;
  title: string;
  prompt: string;
  status: string;
  executor?: string;
  scheduler_enabled: boolean;
  scheduler_config?: string;
  tag_names: string[];
  workspace_path?: string;
  action: 'new' | 'overwrite';
  existingTitle?: string;
}

export function ImportExportModals({
  wizardOpen, setWizardOpen, handleWizardConfirm, importing,
  selectedRowKeys, setSelectedRowKeys, wizardItems,
  exportModalOpen, setExportModalOpen, handleExportSelected,
  exportingSelected, exportTodoKeys, setExportTodoKeys, todos,
  // 导入目标工作空间选择
  workspaces, importWorkspaceId, setImportWorkspaceId,
  // 原始工作空间提示（从备份文件检测到后展示，帮助用户判断）
  sourceWorkspaceInfo,
}: {
  wizardOpen: boolean;
  setWizardOpen: (v: boolean) => void;
  handleWizardConfirm: () => Promise<void>;
  importing: boolean;
  selectedRowKeys: number[];
  setSelectedRowKeys: (keys: number[]) => void;
  wizardItems: ImportItem[];
  exportModalOpen: boolean;
  setExportModalOpen: (v: boolean) => void;
  handleExportSelected: () => Promise<void>;
  exportingSelected: boolean;
  exportTodoKeys: number[];
  setExportTodoKeys: (keys: number[]) => void;
  todos: readonly any[];
  // 导入目标工作空间选择
  workspaces: any[];
  importWorkspaceId: number | null;
  setImportWorkspaceId: (v: number | null) => void;
  // 原始工作空间提示（从备份文件检测到后展示，帮助用户判断）
  sourceWorkspaceInfo?: { id: number; path: string } | null;
}) {
  return (
    <>
      <Modal
        title="导入预览"
        open={wizardOpen}
        onCancel={() => setWizardOpen(false)}
        onOk={handleWizardConfirm}
        okText={`导入 ${selectedRowKeys.length} 项`}
        cancelText="取消"
        confirmLoading={importing}
        width={800}
        okButtonProps={{ disabled: selectedRowKeys.length === 0 || !importWorkspaceId }}
      >
        <div style={{ marginBottom: 12, display: 'flex', gap: 16 }}>
          <AntTag color="green">{wizardItems.filter(i => i.action === 'new').length} 个新建</AntTag>
          <AntTag color="orange">{wizardItems.filter(i => i.action === 'overwrite').length} 个覆盖</AntTag>
          <AntTag color="blue">已选 {selectedRowKeys.length} 项</AntTag>
        </div>

        {/* 原始工作空间提示：检测到备份文件中的工作空间后，显示给用户参考 */}
        {sourceWorkspaceInfo && (() => {
          const matched = workspaces.find((w: any) => w.id === sourceWorkspaceInfo.id);
          return (
            <Alert
              message="检测到原始工作空间"
              description={
                matched
                  ? `该文件中的数据原本来自工作空间「${matched.name || matched.path}」${
                      importWorkspaceId === sourceWorkspaceInfo.id ? '（已自动匹配）' : ''
                    }`
                  : `该文件中的数据原本来自工作空间 ID=${sourceWorkspaceInfo.id}${
                      sourceWorkspaceInfo.path ? ` (${sourceWorkspaceInfo.path})` : ''
                    }，当前环境未找到匹配的工作空间`
              }
              type="info"
              showIcon
              icon={<InfoCircleOutlined />}
              style={{ marginBottom: 16 }}
            />
          );
        })()}

        {/* 目标工作空间选择：表格形式总览，一行一个，点选某个 */}
        <div style={{ marginBottom: 16 }}>
          <Typography.Text strong>目标工作空间</Typography.Text>
          <Typography.Paragraph type="secondary" style={{ margin: '4px 0 8px', fontSize: 12 }}>
            选择导入后 Todo 所属的工作空间，此操作将覆盖备份文件中的原始工作空间信息
          </Typography.Paragraph>
          <Table
            size="small"
            pagination={false}
            rowKey="id"
            dataSource={workspaces.map((w: any) => ({
              ...w,
              // 标记是否为导出文件中的原始工作空间
              _isOriginal: sourceWorkspaceInfo?.id === w.id,
            }))}
            rowSelection={{
              type: 'radio',
              selectedRowKeys: importWorkspaceId != null ? [importWorkspaceId] : [],
              onChange: (keys) => {
                if (keys.length > 0) setImportWorkspaceId(keys[0] as number);
              },
            }}
            columns={[
              {
                title: '工作空间',
                dataIndex: 'name',
                width: '60%',
                render: (_: any, r: any) => r.name || r.path || '(未命名)',
              },
              {
                title: '来源',
                dataIndex: '_isOriginal',
                width: 60,
                render: (v: boolean) => v ? <AntTag color="blue">原始</AntTag> : null,
              },
            ]}
          />
        </div>

        <Divider style={{ margin: '12px 0' }} />

        <Table
          dataSource={wizardItems}
          rowKey="key"
          size="small"
          pagination={false}
          scroll={{ y: 350 }}
          rowSelection={{
            selectedRowKeys,
            onChange: (keys) => setSelectedRowKeys(keys as number[]),
          }}
          columns={[
            {
              title: '标题',
              dataIndex: 'title',
              ellipsis: true,
              width: '35%',
            },
            {
              title: '状态',
              dataIndex: 'action',
              width: 80,
              render: (action: 'new' | 'overwrite') => (
                <AntTag color={action === 'new' ? 'green' : 'orange'}>
                  {action === 'new' ? '新建' : '覆盖'}
                </AntTag>
              ),
            },
            {
              title: '执行器',
              dataIndex: 'executor',
              width: 100,
              render: (v: string | undefined) => v || '-',
            },
            {
              title: '标签',
              dataIndex: 'tag_names',
              width: 150,
              render: (names: string[]) => names.length > 0
                ? names.slice(0, 3).map(n => <AntTag key={n}>{n}</AntTag>)
                : '-',
            },
            {
              title: 'Prompt 摘要',
              dataIndex: 'prompt',
              ellipsis: true,
              render: (v: string) => v ? v.slice(0, 60) + (v.length > 60 ? '...' : '') : '-',
            },
          ]}
        />
      </Modal>

      <Modal
        title="选择性导出"
        open={exportModalOpen}
        onCancel={() => setExportModalOpen(false)}
        onOk={handleExportSelected}
        okText={`导出 ${exportTodoKeys.length} 项`}
        cancelText="取消"
        confirmLoading={exportingSelected}
        width={700}
        okButtonProps={{ disabled: exportTodoKeys.length === 0 }}
      >
        <Table
          dataSource={todos}
          rowKey="id"
          size="small"
          pagination={{ pageSize: 50 }}
          scroll={{ y: 400 }}
          rowSelection={{
            selectedRowKeys: exportTodoKeys,
            onChange: (keys) => setExportTodoKeys(keys as number[]),
          }}
          columns={[
            {
              title: '标题',
              dataIndex: 'title',
              ellipsis: true,
            },
            {
              title: '执行器',
              dataIndex: 'executor',
              width: 100,
              render: (v: string | undefined) => v || '-',
            },
            {
              title: '状态',
              dataIndex: 'status',
              width: 80,
              render: (v: string) => {
                const map: Record<string, { color: string; label: string }> = {
                  pending: { color: 'default', label: '待办' },
                  running: { color: 'processing', label: '进行中' },
                  completed: { color: 'success', label: '完成' },
                  failed: { color: 'error', label: '失败' },
                };
                const s = map[v] || { color: 'default', label: v };
                return <AntTag color={s.color}>{s.label}</AntTag>;
              },
            },
          ]}
        />
      </Modal>
    </>
  );
}