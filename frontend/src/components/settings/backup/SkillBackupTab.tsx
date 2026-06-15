import { Card, Button, Switch, InputNumber, List, Popconfirm, Space, Typography } from 'antd';
import { DownloadOutlined, DatabaseOutlined, ClockCircleOutlined, DeleteOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CronPresetSelect } from '@/components/CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '@/utils/cron';
import { formatFileSize } from '@/utils/format';

interface ExecutorSkillInfo {
  executor: string;
  skills_count: number;
  skills_dir_exists: boolean;
}

interface SkillBackupFilesInfo {
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
  executor_skills: ExecutorSkillInfo[];
}

export function SkillBackupTab({
  skillBackupStatus, autoSkillBackupEnabled, autoSkillBackupCron, autoSkillBackupMaxFiles, skillBackupLoading,
  setAutoSkillBackupEnabled, setAutoSkillBackupCron, setAutoSkillBackupMaxFiles,
  onTriggerBackup, onSaveAutoBackup, onDeleteBackup, onDownloadBackupFile,
}: {
  skillBackupStatus: SkillBackupFilesInfo | null;
  autoSkillBackupEnabled: boolean;
  autoSkillBackupCron: string;
  autoSkillBackupMaxFiles: number;
  skillBackupLoading: boolean;
  setAutoSkillBackupEnabled: (v: boolean) => void;
  setAutoSkillBackupCron: (v: string) => void;
  setAutoSkillBackupMaxFiles: (v: number) => void;
  onTriggerBackup: () => Promise<void>;
  onSaveAutoBackup: () => Promise<void>;
  onDeleteBackup: (filename: string) => Promise<void>;
  onDownloadBackupFile: (filename: string) => void;
}) {
  return (
    <div style={{ maxWidth: 600 }}>
      <Card title="Skill备份" size="small">
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <Typography.Paragraph type="secondary">
            备份各执行器下的 skills 文件夹，包含所有自定义和内置技能
          </Typography.Paragraph>
          <div style={{ display: 'flex', gap: 8 }}>
            <Button icon={<DatabaseOutlined />} onClick={onTriggerBackup} loading={skillBackupLoading}>
              立即备份
            </Button>
          </div>

          {skillBackupStatus && skillBackupStatus.executor_skills.length > 0 && (
            <div style={{ marginTop: 12, padding: '12px 16px', background: 'var(--color-bg-secondary)', borderRadius: 8 }}>
              <div style={{ fontWeight: 600, marginBottom: 8 }}>执行器 Skills 概览</div>
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 8 }}>
                {skillBackupStatus.executor_skills.map((executor) => (
                  <div key={executor.executor} style={{ display: 'flex', justifyContent: 'space-between', fontSize: 13 }}>
                    <span style={{ color: executor.skills_dir_exists ? 'var(--color-text-primary)' : 'var(--color-text-tertiary)' }}>
                      {executor.executor}
                    </span>
                    <span style={{ color: executor.skills_dir_exists ? 'var(--color-text-secondary)' : 'var(--color-text-tertiary)' }}>
                      {executor.skills_dir_exists ? `${executor.skills_count} skills` : '目录不存在'}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12, marginTop: 4 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
              <span style={{ fontWeight: 600 }}><ClockCircleOutlined style={{ marginRight: 6 }} />自动备份</span>
              <Switch checked={autoSkillBackupEnabled} onChange={setAutoSkillBackupEnabled} />
            </div>
            {autoSkillBackupEnabled && (
              <CronPresetSelect value={autoSkillBackupCron} onChange={(val) => setAutoSkillBackupCron(val)} />
            )}
            {autoSkillBackupEnabled && (
              <Cron
                value={cronTo5(autoSkillBackupCron)}
                setValue={(val: string) => { setAutoSkillBackupCron(cronTo6(val)); }}
                locale={CRON_ZH_LOCALE}
                defaultPeriod="day"
                humanizeLabels
                allowClear={false}
              />
            )}
            {autoSkillBackupEnabled && (
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
                <span style={{ fontSize: 12, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>保留数量</span>
                <InputNumber min={1} max={1000} value={autoSkillBackupMaxFiles} onChange={(v) => v && setAutoSkillBackupMaxFiles(v)} style={{ width: 80 }} size="small" />
                <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>个备份文件</span>
              </div>
            )}
            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
              <Button size="small" type="primary" onClick={onSaveAutoBackup} loading={skillBackupLoading}>
                保存
              </Button>
            </div>
          </div>

          {skillBackupStatus && skillBackupStatus.files.length > 0 && (
            <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12 }}>
              <div style={{ fontWeight: 600, marginBottom: 8 }}>备份文件 ({skillBackupStatus.files.length})</div>
              <List
                size="small"
                dataSource={skillBackupStatus.files}
                renderItem={(file) => (
                  <List.Item style={{ padding: '6px 0', fontSize: 12 }}>
                    <div>
                      <div style={{ fontWeight: 500 }}>{file.name}</div>
                      <div style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>
                        {formatFileSize(file.size)} · {file.created_at}
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
