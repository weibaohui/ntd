import { Card, Button, Switch, InputNumber, List, Popconfirm, Space, Typography } from 'antd';
import { DownloadOutlined, DatabaseOutlined, SettingOutlined, ClockCircleOutlined, DeleteOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CronPresetSelect } from '../../CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '../../../utils/cron';

interface BackupFilesInfo {
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
}

export function DatabaseBackupTab({
  backupStatus, autoBackupEnabled, autoBackupCron, autoBackupMaxFiles, backupLoading,
  logCleanupDays,
  setAutoBackupEnabled, setAutoBackupCron, setAutoBackupMaxFiles, setLogCleanupDays,
  onTriggerBackup, onSaveAutoBackup, onDeleteBackup, onDownloadBackupFile,
  onDownloadDatabase, onOptimizeDatabase,
  onSaveLogCleanup, onTriggerLogCleanup,
}: {
  backupStatus: BackupFilesInfo | null;
  autoBackupEnabled: boolean;
  autoBackupCron: string;
  autoBackupMaxFiles: number;
  backupLoading: boolean;
  logCleanupDays: number | null;
  setAutoBackupEnabled: (v: boolean) => void;
  setAutoBackupCron: (v: string) => void;
  setAutoBackupMaxFiles: (v: number) => void;
  setLogCleanupDays: (v: number | null) => void;
  onTriggerBackup: () => Promise<void>;
  onSaveAutoBackup: () => Promise<void>;
  onDeleteBackup: (filename: string) => Promise<void>;
  onDownloadBackupFile: (filename: string) => void;
  onDownloadDatabase: () => Promise<void>;
  onOptimizeDatabase: () => Promise<void>;
  onSaveLogCleanup: () => Promise<void>;
  onTriggerLogCleanup: () => Promise<void>;
}) {
  return (
    <div style={{ maxWidth: 600 }}>
      <Card title="数据库备份" size="small">
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <Typography.Paragraph type="secondary">
            直接备份 SQLite 数据库文件，包含所有数据（含执行记录）
          </Typography.Paragraph>
          <div style={{ display: 'flex', gap: 8 }}>
            <Button icon={<DownloadOutlined />} onClick={onDownloadDatabase}>
              下载数据库
            </Button>
            <Button icon={<DatabaseOutlined />} onClick={onTriggerBackup} loading={backupLoading}>
              备份到服务器
            </Button>
            <Button icon={<SettingOutlined />} onClick={onOptimizeDatabase} loading={backupLoading}>
              压缩优化
            </Button>
          </div>

          <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12, marginTop: 4 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
              <span style={{ fontWeight: 600 }}><ClockCircleOutlined style={{ marginRight: 6 }} />自动备份</span>
              <Switch checked={autoBackupEnabled} onChange={setAutoBackupEnabled} />
            </div>
            {autoBackupEnabled && (
              <CronPresetSelect value={autoBackupCron} onChange={(val) => setAutoBackupCron(val)} />
            )}
            {autoBackupEnabled && (
              <Cron
                value={cronTo5(autoBackupCron)}
                setValue={(val: string) => { setAutoBackupCron(cronTo6(val)); }}
                locale={CRON_ZH_LOCALE}
                defaultPeriod="day"
                humanizeLabels
                allowClear={false}
              />
            )}
            {autoBackupEnabled && (
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
                <span style={{ fontSize: 12, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>保留数量</span>
                <InputNumber min={1} max={1000} value={autoBackupMaxFiles} onChange={(v) => v && setAutoBackupMaxFiles(v)} style={{ width: 80 }} size="small" />
                <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>个备份文件</span>
              </div>
            )}
            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
              <Button size="small" type="primary" onClick={onSaveAutoBackup} loading={backupLoading}>
                保存
              </Button>
            </div>
          </div>

          {backupStatus && backupStatus.files.length > 0 && (
            <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12 }}>
              <div style={{ fontWeight: 600, marginBottom: 8 }}>备份文件 ({backupStatus.files.length})</div>
              <List
                size="small"
                dataSource={backupStatus.files}
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

      <Card title="清理日志" size="small" style={{ marginTop: 16 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <Typography.Paragraph type="secondary">
            清理 execution_logs 表中早于指定天数的日志记录，释放数据库空间
          </Typography.Paragraph>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>保留日志</span>
            <InputNumber min={1} max={365} value={logCleanupDays ?? undefined} onChange={(v) => setLogCleanupDays(v ?? null)} style={{ width: 80 }} size="small" />
            <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>天</span>
            <Button size="small" type="primary" onClick={onSaveLogCleanup} style={{ marginLeft: 8 }}>
              保存
            </Button>
            <Button size="small" onClick={onTriggerLogCleanup} style={{ marginLeft: 4 }}>
              立即清理
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
