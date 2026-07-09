import { Modal, Table, Tag as AntTag, Divider, Alert, Select, Button } from 'antd';
import { InfoCircleOutlined } from '@ant-design/icons';
import type { ProjectDirectory } from '@/utils/database/todos';
import { WorkspaceSwitcher } from '@/components/shell/WorkspaceSwitcher';

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
    workspace_id?: number;
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
  /** 导出文件里的原始工作空间 ID，用于按行默认匹配与「来源」展示 */
  workspace_id?: number | null;
  /** 同名已存在：true 时该条可在「覆盖/跳过」间切换；false 为纯新建 */
  exists?: boolean;
  action: 'new' | 'overwrite' | 'skip';
  existingTitle?: string;
}

/** 按行判断原始工作空间是否在当前环境命中 */
function isSourceMatched(item: ImportItem, workspaces: ProjectDirectory[]): boolean {
  return item.workspace_id != null && workspaces.some((w) => w.id === item.workspace_id);
}

export function ImportExportModals({
  wizardOpen, setWizardOpen, handleWizardConfirm, importing,
  selectedRowKeys, setSelectedRowKeys, wizardItems,
  // 同名项动作批量/逐条设置（覆盖/跳过）
  setItemsAction,
  // 逐行工作空间选择
  workspaces, rowWorkspaceMap, setRowWorkspaceId,
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
  // 同名项动作批量/逐条设置（覆盖/跳过）
  setItemsAction: (keys: number[], action: 'overwrite' | 'skip') => void;
  // 逐行工作空间选择：key → workspaceId（null=未指定，需用户手选）
  workspaces: ProjectDirectory[];
  rowWorkspaceMap: Record<number, number | null>;
  setRowWorkspaceId: (key: number, id: number | null) => void;
  // 原始工作空间提示（从备份文件检测到后展示，帮助用户判断）
  sourceWorkspaceInfo?: { id: number; path: string } | null;
}) {
  // 同名项 keys（exists=true）：批量按钮与可编辑「同名处理」列的作用范围
  const sameNameKeys = wizardItems.filter((i) => i.exists).map((i) => i.key);
  // 实际将导入的项：已勾选 且 action 非 skip（skip 一律不提交）
  const willImportItems = wizardItems.filter(
    (i) => selectedRowKeys.includes(i.key) && i.action !== 'skip',
  );
  const willImportCount = willImportItems.length;
  // OK gate：至少有一项将导入，且每条将导入的 todo 都已指定工作空间
  const hasUnassigned = willImportItems.some((i) => rowWorkspaceMap[i.key] == null);
  return (
    <>
      <Modal
        title="导入预览"
        open={wizardOpen}
        onCancel={() => setWizardOpen(false)}
        onOk={handleWizardConfirm}
        okText={`导入 ${willImportCount} 项`}
        cancelText="取消"
        confirmLoading={importing}
        width={900}
        okButtonProps={{ disabled: willImportCount === 0 || hasUnassigned }}
      >
        <div style={{ marginBottom: 12, display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
          <AntTag color="green">{wizardItems.filter(i => i.action === 'new').length} 新建</AntTag>
          <AntTag color="orange">{wizardItems.filter(i => i.action === 'overwrite').length} 覆盖</AntTag>
          <AntTag>{wizardItems.filter(i => i.action === 'skip').length} 跳过</AntTag>
          <AntTag color="blue">将导入 {willImportCount} 项</AntTag>
          <Button size="small" disabled={sameNameKeys.length === 0} onClick={() => setItemsAction(sameNameKeys, 'overwrite')}>同名全覆盖</Button>
          <Button size="small" disabled={sameNameKeys.length === 0} onClick={() => setItemsAction(sameNameKeys, 'skip')}>同名全跳过</Button>
        </div>

        {/* 原始工作空间提示：检测到备份文件中的工作空间后，显示给用户参考 */}
        {sourceWorkspaceInfo && (() => {
          const matched = workspaces.find((w) => w.id === sourceWorkspaceInfo.id);
          return (
            <Alert
              message="检测到原始工作空间"
              description={
                matched
                  ? `该文件中的数据原本来自工作空间「${matched.name || matched.path}」，已按行自动匹配；可逐条点改`
                  : `该文件中的数据原本来自工作空间 ID=${sourceWorkspaceInfo.id}${
                      sourceWorkspaceInfo.path ? ` (${sourceWorkspaceInfo.path})` : ''
                    }，当前环境未找到匹配的工作空间，请逐行指定`
              }
              type="info"
              showIcon
              icon={<InfoCircleOutlined />}
              style={{ marginBottom: 16 }}
            />
          );
        })()}

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
              width: '22%',
            },
            {
              title: '同名处理',
              width: 104,
              // 同名项(exists)可在「覆盖/跳过」间切换；纯新建项固定显示「新建」
              render: (_: unknown, r: ImportItem) =>
                r.exists ? (
                  <Select
                    size="small"
                    value={r.action === 'overwrite' ? 'overwrite' : 'skip'}
                    onChange={(v) => setItemsAction([r.key], v as 'overwrite' | 'skip')}
                    style={{ width: 96 }}
                    options={[
                      { value: 'overwrite', label: '覆盖' },
                      { value: 'skip', label: '跳过' },
                    ]}
                  />
                ) : (
                  <AntTag color="green">新建</AntTag>
                ),
            },
            {
              title: '工作空间',
              width: 180,
              // 逐行工作空间选择：默认按原 id 匹配，匹配不上为 null（未匹配），可点改
              render: (_: unknown, r: ImportItem) => (
                <WorkspaceSwitcher
                  dirs={workspaces}
                  value={rowWorkspaceMap[r.key] ?? null}
                  showAddOption={false}
                  onChange={(id) => setRowWorkspaceId(r.key, id)}
                />
              ),
            },
            {
              title: '来源',
              width: 80,
              // 原始工作空间命中→「原始」；有原 id 但当前库不存在→「未匹配」；无原 id→'-'
              render: (_: unknown, r: ImportItem) => {
                if (r.workspace_id == null) return '-';
                return isSourceMatched(r, workspaces)
                  ? <AntTag color="blue">原始</AntTag>
                  : <AntTag color="red">未匹配</AntTag>;
              },
            },
            {
              title: '执行器',
              dataIndex: 'executor',
              width: 90,
              render: (v: string | undefined) => v || '-',
            },
            {
              title: '标签',
              dataIndex: 'tag_names',
              width: 120,
              render: (names: string[]) => names.length > 0
                ? names.slice(0, 3).map(n => <AntTag key={n}>{n}</AntTag>)
                : '-',
            },
            {
              title: 'Prompt 摘要',
              dataIndex: 'prompt',
              ellipsis: true,
              render: (v: string) => v ? v.slice(0, 40) + (v.length > 40 ? '...' : '') : '-',
            },
          ]}
        />
      </Modal>
    </>
  );
}
