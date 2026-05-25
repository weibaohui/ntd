import { Card, Button, Switch, InputNumber, List, Popconfirm, Space, Typography, Upload } from 'antd';
import { DownloadOutlined, DatabaseOutlined, ClockCircleOutlined, InboxOutlined, DeleteOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CronPresetSelect } from '../../CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '../../../utils/cron';

const { Dragger } = Upload;

interface BackupFilesInfo {
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
}

export function TodoBackupTab({
  todoBackupStatus, autoTodoBackupEnabled, autoTodoBackupCron, autoTodoBackupMaxFiles, todoBackupLoading,
  setAutoTodoBackupEnabled, setAutoTodoBackupCron, setAutoTodoBackupMaxFiles,
  onTriggerBackup, onSaveAutoBackup, onDeleteBackup, onDownloadBackupFile,
  onExportBackup, onOpenExportModal, onImportFile,
}: {
  todoBackupStatus: BackupFilesInfo | null;
  autoTodoBackupEnabled: boolean;
  autoTodoBackupCron: string;
  autoTodoBackupMaxFiles: number;
  todoBackupLoading: boolean;
  setAutoTodoBackupEnabled: (v: boolean) => void;
  setAutoTodoBackupCron: (v: string) => void;
  setAutoTodoBackupMaxFiles: (v: number) => void;
  onTriggerBackup: () => Promise<void>;
  onSaveAutoBackup: () => Promise<void>;
  onDeleteBackup: (filename: string) => Promise<void>;
  onDownloadBackupFile: (filename: string) => void;
  onExportBackup: () => Promise<void>;
  onOpenExportModal: () => void;
  onImportFile: (file: File) => Promise<boolean>;
}) {
  return (
    <div style={{ maxWidth: 600 }}>
      <Card title="导出备份" size="small" style={{ marginBottom: 24 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <Typography.Paragraph type="secondary">
            将 Todo 和标签导出为 YAML 文件，方便迁移和存档
          </Typography.Paragraph>
          <div style={{ display: 'flex', gap: 8 }}>
            <Button type="primary" icon={<DownloadOutlined />} onClick={onExportBackup} style={{ flex: 1 }}>
              导出全部
            </Button>
            <Button icon={<DownloadOutlined />} onClick={onOpenExportModal} style={{ flex: 1 }}>
              选择性导出
            </Button>
          </div>
        </div>
      </Card>

      <Card title="导入备份" size="small" style={{ marginBottom: 24 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <Typography.Paragraph type="secondary">
            从 YAML 文件恢复数据，支持预览和选择性导入
          </Typography.Paragraph>
          <Dragger
            accept=".yaml,.yml"
            beforeUpload={onImportFile}
            showUploadList={false}
            style={{ borderRadius: 12 }}
          >
            <p className="ant-upload-drag-icon">
              <InboxOutlined style={{ color: '#0891b2' }} />
            </p>
            <p className="ant-upload-text">点击或拖拽 YAML 文件到此处</p>
            <p className="ant-upload-hint">将解析文件并展示预览，可选择性导入</p>
          </Dragger>
        </div>
      </Card>

      <Card title="Todo自动备份" size="small">
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <Typography.Paragraph type="secondary">
            将 Todo 和标签打包备份到服务器，支持定时自动备份
          </Typography.Paragraph>
          <div style={{ display: 'flex', gap: 8 }}>
            <Button icon={<DatabaseOutlined />} onClick={onTriggerBackup} loading={todoBackupLoading}>
              立即备份
            </Button>
          </div>

          <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12, marginTop: 4 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
              <span style={{ fontWeight: 600 }}><ClockCircleOutlined style={{ marginRight: 6 }} />自动备份</span>
              <Switch checked={autoTodoBackupEnabled} onChange={setAutoTodoBackupEnabled} />
            </div>
            {autoTodoBackupEnabled && (
              <CronPresetSelect value={autoTodoBackupCron} onChange={(val) => setAutoTodoBackupCron(val)} />
            )}
            {autoTodoBackupEnabled && (
              <Cron
                value={cronTo5(autoTodoBackupCron)}
                setValue={(val: string) => { setAutoTodoBackupCron(cronTo6(val)); }}
                locale={CRON_ZH_LOCALE}
                defaultPeriod="day"
                humanizeLabels
                allowClear={false}
              />
            )}
            {autoTodoBackupEnabled && (
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
                <span style={{ fontSize: 12, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>保留数量</span>
                <InputNumber min={1} max={1000} value={autoTodoBackupMaxFiles} onChange={(v) => v && setAutoTodoBackupMaxFiles(v)} style={{ width: 80 }} size="small" />
                <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>个备份文件</span>
              </div>
            )}
            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
              <Button size="small" type="primary" onClick={onSaveAutoBackup} loading={todoBackupLoading}>
                保存
              </Button>
            </div>
          </div>

          {todoBackupStatus && todoBackupStatus.files.length > 0 && (
            <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12 }}>
              <div style={{ fontWeight: 600, marginBottom: 8 }}>备份文件 ({todoBackupStatus.files.length})</div>
              <List
                size="small"
                dataSource={todoBackupStatus.files}
                renderItem={(file) => (
                  <List.Item style={{ padding: '6px 0', fontSize: 12 }}>
                    <div>
                      <div style={{ fontWeight: 500 }}>{file.name}</div>
                      <div style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>
                        {(file.size / 1024).toFixed(1)} KB · {file.created_at}
                      </div>
                    </div>
                    <Space size={4}>
                      <Button type="text" icon={<DownloadOutlined />} size="small" onClick={() => onDownloadBackupFile(file.name)} />
                      <Popconfirm title="确定删除此备份？" onConfirm={() => onDeleteBackup(file.name)}>
                        <Button type="text" danger icon={<DeleteOutlined />} size="small" />
                      </Popconfirm>
                    </Space>
                  </List.Item>
                )}
              />
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}
