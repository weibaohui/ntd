import { useState, useEffect } from 'react';
import { Tabs, Card, Button, Switch, InputNumber, List, Popconfirm, Space, Typography, message, Upload, Table, Tag as AntTag, Modal } from 'antd';
import { DownloadOutlined, DatabaseOutlined, ClockCircleOutlined, SettingOutlined, DeleteOutlined, InboxOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { useApp } from '../../hooks/useApp';
import { CronPresetSelect } from '../CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '../../utils/cron';
import * as db from '../../utils/database';
import yaml from 'js-yaml';

const { Dragger } = Upload;

interface BackupDataYaml {
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
    workspace?: string;
  }[];
}

interface ImportItem {
  key: number;
  title: string;
  prompt: string;
  status: string;
  executor?: string;
  scheduler_enabled: boolean;
  scheduler_config?: string;
  tag_names: string[];
  workspace?: string;
  action: 'new' | 'overwrite';
  existingTitle?: string;
}

export function BackupPanel() {
  const { state } = useApp();
  const { todos } = state;
  // Database backup state
  const [backupStatus, setBackupStatus] = useState<{
    auto_backup_enabled: boolean;
    auto_backup_cron: string;
    auto_backup_max_files: number;
    last_backup: string | null;
    files: { name: string; size: number; created_at: string }[];
  } | null>(null);
  const [autoBackupEnabled, setAutoBackupEnabled] = useState(false);
  const [autoBackupCron, setAutoBackupCron] = useState('0 0 3 * * *');
  const [autoBackupMaxFiles, setAutoBackupMaxFiles] = useState(30);
  const [backupLoading, setBackupLoading] = useState(false);
  const [logCleanupDays, setLogCleanupDays] = useState<number | null>(30);

  // Todo backup state
  const [todoBackupStatus, setTodoBackupStatus] = useState<{
    auto_backup_enabled: boolean;
    auto_backup_cron: string;
    auto_backup_max_files: number;
    last_backup: string | null;
    files: { name: string; size: number; created_at: string }[];
  } | null>(null);
  const [autoTodoBackupEnabled, setAutoTodoBackupEnabled] = useState(false);
  const [autoTodoBackupCron, setAutoTodoBackupCron] = useState('0 0 4 * * *');
  const [autoTodoBackupMaxFiles, setAutoTodoBackupMaxFiles] = useState(30);
  const [todoBackupLoading, setTodoBackupLoading] = useState(false);

  // Skill backup state
  const [skillBackupStatus, setSkillBackupStatus] = useState<{
    auto_backup_enabled: boolean;
    auto_backup_cron: string;
    auto_backup_max_files: number;
    last_backup: string | null;
    files: { name: string; size: number; created_at: string }[];
    executor_skills: { executor: string; skills_count: number; skills_dir_exists: boolean }[];
  } | null>(null);
  const [autoSkillBackupEnabled, setAutoSkillBackupEnabled] = useState(false);
  const [autoSkillBackupCron, setAutoSkillBackupCron] = useState('0 0 5 * * *');
  const [autoSkillBackupMaxFiles, setAutoSkillBackupMaxFiles] = useState(30);
  const [skillBackupLoading, setSkillBackupLoading] = useState(false);

  // Import/Export state
  const [importing, setImporting] = useState(false);
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportTodoKeys, setExportTodoKeys] = useState<number[]>([]);
  const [exportingSelected, setExportingSelected] = useState(false);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [wizardItems, setWizardItems] = useState<ImportItem[]>([]);
  const [wizardTags, setWizardTags] = useState<{ name: string; color: string }[]>([]);
  const [selectedRowKeys, setSelectedRowKeys] = useState<number[]>([]);

  // Load status
  useEffect(() => {
    db.getDatabaseBackupStatus()
      .then((status) => {
        setBackupStatus(status);
        setAutoBackupEnabled(status.auto_backup_enabled);
        setAutoBackupCron(status.auto_backup_cron);
        setAutoBackupMaxFiles(status.auto_backup_max_files);
      })
      .catch(() => {});

    db.getLogCleanupStatus()
      .then((status) => {
        setLogCleanupDays(status.cleanup_days);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    db.getTodoBackupStatus()
      .then((status) => {
        setTodoBackupStatus(status);
        setAutoTodoBackupEnabled(status.auto_backup_enabled);
        setAutoTodoBackupCron(status.auto_backup_cron);
        setAutoTodoBackupMaxFiles(status.auto_backup_max_files);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    db.getSkillBackupStatus()
      .then((status) => {
        setSkillBackupStatus(status);
        setAutoSkillBackupEnabled(status.auto_backup_enabled);
        setAutoSkillBackupCron(status.auto_backup_cron);
        setAutoSkillBackupMaxFiles(status.auto_backup_max_files);
      })
      .catch(() => {});
  }, []);

  // Handlers - Database backup
  const handleTriggerBackup = async () => {
    setBackupLoading(true);
    try {
      const msg = await db.triggerLocalBackup();
      message.success(msg);
      const status = await db.getDatabaseBackupStatus();
      setBackupStatus(status);
    } catch (err: any) {
      message.error(err?.message || '备份失败');
    } finally {
      setBackupLoading(false);
    }
  };

  const handleOptimizeDatabase = async () => {
    setBackupLoading(true);
    try {
      const msg = await db.optimizeDatabase();
      message.success(msg);
    } catch (err: any) {
      message.error(err?.message || '优化失败');
    } finally {
      setBackupLoading(false);
    }
  };

  const handleDownloadDatabase = async () => {
    try {
      const response = await fetch('/xyz/backup/database/download');
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      a.download = `ntd-database-${timestamp}.db`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success('数据库下载成功');
    } catch (err: any) {
      message.error(err?.message || '下载失败');
    }
  };

  const handleSaveAutoBackup = async () => {
    setBackupLoading(true);
    try {
      await db.updateAutoBackup(autoBackupEnabled, autoBackupCron, autoBackupMaxFiles);
      message.success('自动备份配置已保存');
    } catch (err: any) {
      message.error(err?.message || '保存失败');
    } finally {
      setBackupLoading(false);
    }
  };

  const handleDeleteBackup = async (filename: string) => {
    try {
      await db.deleteBackupFile(filename);
      message.success('已删除');
      const status = await db.getDatabaseBackupStatus();
      setBackupStatus(status);
    } catch (err: any) {
      message.error(err?.message || '删除失败');
    }
  };

  const handleDownloadBackupFile = (filename: string) => {
    const url = db.downloadBackupFileUrl(filename);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  };

  // Log cleanup handlers
  const handleSaveLogCleanup = async () => {
    setBackupLoading(true);
    try {
      await db.updateLogCleanup(logCleanupDays);
      message.success('日志清理配置已保存');
    } catch (err: any) {
      message.error(err?.message || '保存失败');
    } finally {
      setBackupLoading(false);
    }
  };

  const handleTriggerLogCleanup = async () => {
    setBackupLoading(true);
    try {
      const result = await db.triggerLogCleanup();
      message.success(result);
    } catch (err: any) {
      message.error(err?.message || '清理失败');
    } finally {
      setBackupLoading(false);
    }
  };

  // Todo backup handlers
  const handleTriggerTodoBackup = async () => {
    setTodoBackupLoading(true);
    try {
      const msg = await db.triggerTodoBackup();
      message.success(msg);
      const status = await db.getTodoBackupStatus();
      setTodoBackupStatus(status);
    } catch (err: any) {
      message.error(err?.message || '备份失败');
    } finally {
      setTodoBackupLoading(false);
    }
  };

  const handleSaveTodoAutoBackup = async () => {
    setTodoBackupLoading(true);
    try {
      await db.updateTodoAutoBackup(autoTodoBackupEnabled, autoTodoBackupCron, autoTodoBackupMaxFiles);
      message.success('Todo自动备份配置已保存');
    } catch (err: any) {
      message.error(err?.message || '保存失败');
    } finally {
      setTodoBackupLoading(false);
    }
  };

  const handleDeleteTodoBackup = async (filename: string) => {
    try {
      await db.deleteTodoBackupFile(filename);
      message.success('已删除');
      const status = await db.getTodoBackupStatus();
      setTodoBackupStatus(status);
    } catch (err: any) {
      message.error(err?.message || '删除失败');
    }
  };

  const handleDownloadTodoBackupFile = (filename: string) => {
    const url = db.downloadTodoBackupFileUrl(filename);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  };

  // Skill backup handlers
  const handleTriggerSkillBackup = async () => {
    setSkillBackupLoading(true);
    try {
      const msg = await db.triggerSkillBackup();
      message.success(msg);
      const status = await db.getSkillBackupStatus();
      setSkillBackupStatus(status);
    } catch (err: any) {
      message.error(err?.message || '备份失败');
    } finally {
      setSkillBackupLoading(false);
    }
  };

  const handleSaveSkillAutoBackup = async () => {
    setSkillBackupLoading(true);
    try {
      await db.updateSkillAutoBackup(autoSkillBackupEnabled, autoSkillBackupCron, autoSkillBackupMaxFiles);
      message.success('Skill自动备份配置已保存');
    } catch (err: any) {
      message.error(err?.message || '保存失败');
    } finally {
      setSkillBackupLoading(false);
    }
  };

  const handleDeleteSkillBackup = async (filename: string) => {
    try {
      await db.deleteSkillBackupFile(filename);
      message.success('已删除');
      const status = await db.getSkillBackupStatus();
      setSkillBackupStatus(status);
    } catch (err: any) {
      message.error(err?.message || '删除失败');
    }
  };

  const handleDownloadSkillBackupFile = (filename: string) => {
    const url = db.downloadSkillBackupFileUrl(filename);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  };

  // Export handlers
  const handleExportBackup = async () => {
    try {
      const response = await fetch('/xyz/backup/export', {
        headers: { Accept: 'application/x-yaml' },
      });
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      const yamlText = await response.text();
      const blob = new Blob([yamlText], { type: 'application/x-yaml' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      a.download = `aietodo-backup-${timestamp}.yaml`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success('备份导出成功');
    } catch (err: any) {
      message.error(err?.message || '导出失败');
    }
  };

  const handleImportFile = async (file: File) => {
    const text = await file.text();
    try {
      const data = yaml.load(text, { schema: yaml.JSON_SCHEMA }) as BackupDataYaml;
      if (!data.todos || data.todos.length === 0) {
        message.error('备份文件中没有 Todo 数据');
        return false;
      }

      const existingTodos = await db.getAllTodos();
      const existingSet = new Set(existingTodos.map(t => `${t.title}\n${t.prompt}`));

      const items: ImportItem[] = data.todos.map((todo, idx) => {
        const key = `${todo.title}\n${todo.prompt}`;
        const exists = existingSet.has(key);
        const existing = exists ? existingTodos.find(t => `${t.title}\n${t.prompt}` === key) : undefined;
        return {
          key: idx,
          title: todo.title,
          prompt: todo.prompt,
          status: todo.status,
          executor: todo.executor,
          scheduler_enabled: todo.scheduler_enabled,
          scheduler_config: todo.scheduler_config,
          tag_names: todo.tag_names || [],
          workspace: todo.workspace,
          action: exists ? 'overwrite' as const : 'new' as const,
          existingTitle: existing?.title,
        };
      });

      setWizardTags(data.tags || []);
      setWizardItems(items);
      setSelectedRowKeys(items.map(i => i.key));
      setWizardOpen(true);
    } catch (err: any) {
      message.error('解析文件失败: ' + (err?.message || String(err)));
    }
    return false;
  };

  const handleWizardConfirm = async () => {
    if (selectedRowKeys.length === 0) {
      message.warning('请至少选择一项');
      return;
    }
    setImporting(true);
    try {
      const selectedTodos = wizardItems
        .filter(item => selectedRowKeys.includes(item.key))
        .map(({ key, action, existingTitle, ...todo }) => todo);
      const msg = await db.mergeBackup(wizardTags, selectedTodos);
      message.success(msg);
      setWizardOpen(false);
      window.location.reload();
    } catch (err: any) {
      message.error(err?.message || '导入失败');
    } finally {
      setImporting(false);
    }
  };

  const handleExportSelected = async () => {
    if (exportTodoKeys.length === 0) {
      message.warning('请至少选择一项');
      return;
    }
    setExportingSelected(true);
    try {
      const response = await fetch('/xyz/backup/export-selected', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/x-yaml' },
        body: JSON.stringify({ todo_ids: exportTodoKeys }),
      });
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      const yamlText = await response.text();
      const blob = new Blob([yamlText], { type: 'application/x-yaml' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      a.download = `aietodo-backup-selected-${timestamp}.yaml`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success(`已导出 ${exportTodoKeys.length} 项`);
      setExportModalOpen(false);
    } catch (err: any) {
      message.error(err?.message || '导出失败');
    } finally {
      setExportingSelected(false);
    }
  };

  return (
    <div>
      <Tabs
        defaultActiveKey="todo"
        items={[
          {
            key: 'todo',
            label: 'Todo备份',
            children: (
              <div style={{ maxWidth: 600 }}>
                <Card title="导出备份" size="small" style={{ marginBottom: 24 }}>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                    <Typography.Paragraph type="secondary">
                      将 Todo 和标签导出为 YAML 文件，方便迁移和存档
                    </Typography.Paragraph>
                    <div style={{ display: 'flex', gap: 8 }}>
                      <Button
                        type="primary"
                        icon={<DownloadOutlined />}
                        onClick={handleExportBackup}
                        style={{ flex: 1 }}
                      >
                        导出全部
                      </Button>
                      <Button
                        icon={<DownloadOutlined />}
                        onClick={() => setExportModalOpen(true)}
                        style={{ flex: 1 }}
                      >
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
                      beforeUpload={handleImportFile}
                      showUploadList={false}
                      disabled={importing}
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
                      <Button
                        icon={<DatabaseOutlined />}
                        onClick={handleTriggerTodoBackup}
                        loading={todoBackupLoading}
                      >
                        立即备份
                      </Button>
                    </div>

                    <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12, marginTop: 4 }}>
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                        <span style={{ fontWeight: 600 }}><ClockCircleOutlined style={{ marginRight: 6 }} />自动备份</span>
                        <Switch checked={autoTodoBackupEnabled} onChange={setAutoTodoBackupEnabled} />
                      </div>
                      {autoTodoBackupEnabled && (
                        <CronPresetSelect
                          value={autoTodoBackupCron}
                          onChange={(val) => setAutoTodoBackupCron(val)}
                        />
                      )}
                      {autoTodoBackupEnabled && (
                        <Cron
                          value={cronTo5(autoTodoBackupCron)}
                          setValue={(val: string) => {
                            setAutoTodoBackupCron(cronTo6(val));
                          }}
                          locale={CRON_ZH_LOCALE}
                          defaultPeriod="day"
                          humanizeLabels
                          allowClear={false}
                        />
                      )}
                      {autoTodoBackupEnabled && (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
                          <span style={{ fontSize: 12, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>保留数量</span>
                          <InputNumber
                            min={1}
                            max={1000}
                            value={autoTodoBackupMaxFiles}
                            onChange={(v) => v && setAutoTodoBackupMaxFiles(v)}
                            style={{ width: 80 }}
                            size="small"
                          />
                          <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>个备份文件</span>
                        </div>
                      )}
                      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
                        <Button size="small" type="primary" onClick={handleSaveTodoAutoBackup} loading={todoBackupLoading}>
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
                            <List.Item
                              style={{ padding: '6px 0', fontSize: 12 }}
                            >
                              <div>
                                <div style={{ fontWeight: 500 }}>{file.name}</div>
                                <div style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>
                                  {(file.size / 1024).toFixed(1)} KB · {file.created_at}
                                </div>
                              </div>
                              <Space size={4}>
                                <Button type="text" icon={<DownloadOutlined />} size="small" onClick={() => handleDownloadTodoBackupFile(file.name)} />
                                <Popconfirm title="确定删除此备份？" onConfirm={() => handleDeleteTodoBackup(file.name)}>
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
            ),
          },
          {
            key: 'skill-backup',
            label: 'Skill备份',
            children: (
              <div style={{ maxWidth: 600 }}>
                <Card title="Skill备份" size="small">
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
                    <Typography.Paragraph type="secondary">
                      备份各执行器下的 skills 文件夹，包含所有自定义和内置技能
                    </Typography.Paragraph>
                    <div style={{ display: 'flex', gap: 8 }}>
                      <Button
                        icon={<DatabaseOutlined />}
                        onClick={handleTriggerSkillBackup}
                        loading={skillBackupLoading}
                      >
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
                        <CronPresetSelect
                          value={autoSkillBackupCron}
                          onChange={(val) => setAutoSkillBackupCron(val)}
                        />
                      )}
                      {autoSkillBackupEnabled && (
                        <Cron
                          value={cronTo5(autoSkillBackupCron)}
                          setValue={(val: string) => {
                            setAutoSkillBackupCron(cronTo6(val));
                          }}
                          locale={CRON_ZH_LOCALE}
                          defaultPeriod="day"
                          humanizeLabels
                          allowClear={false}
                        />
                      )}
                      {autoSkillBackupEnabled && (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
                          <span style={{ fontSize: 12, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>保留数量</span>
                          <InputNumber
                            min={1}
                            max={1000}
                            value={autoSkillBackupMaxFiles}
                            onChange={(v) => v && setAutoSkillBackupMaxFiles(v)}
                            style={{ width: 80 }}
                            size="small"
                          />
                          <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>个备份文件</span>
                        </div>
                      )}
                      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
                        <Button size="small" type="primary" onClick={handleSaveSkillAutoBackup} loading={skillBackupLoading}>
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
                            <List.Item
                              style={{ padding: '6px 0', fontSize: 12 }}
                            >
                              <div>
                                <div style={{ fontWeight: 500 }}>{file.name}</div>
                                <div style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>
                                  {(file.size / 1024).toFixed(1)} KB · {file.created_at}
                                </div>
                              </div>
                              <Space size={4}>
                                <Button type="text" icon={<DownloadOutlined />} size="small" onClick={() => handleDownloadSkillBackupFile(file.name)} />
                                <Popconfirm title="确定删除此备份？" onConfirm={() => handleDeleteSkillBackup(file.name)}>
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
            ),
          },
          {
            key: 'database',
            label: '数据库备份',
            children: (
              <div style={{ maxWidth: 600 }}>
                <Card title="数据库备份" size="small">
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
                    <Typography.Paragraph type="secondary">
                      直接备份 SQLite 数据库文件，包含所有数据（含执行记录）
                    </Typography.Paragraph>
                    <div style={{ display: 'flex', gap: 8 }}>
                      <Button
                        icon={<DownloadOutlined />}
                        onClick={handleDownloadDatabase}
                      >
                        下载数据库
                      </Button>
                      <Button
                        icon={<DatabaseOutlined />}
                        onClick={handleTriggerBackup}
                        loading={backupLoading}
                      >
                        备份到服务器
                      </Button>
                      <Button
                        icon={<SettingOutlined />}
                        onClick={handleOptimizeDatabase}
                        loading={backupLoading}
                      >
                        压缩优化
                      </Button>
                    </div>

                    <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12, marginTop: 4 }}>
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                        <span style={{ fontWeight: 600 }}><ClockCircleOutlined style={{ marginRight: 6 }} />自动备份</span>
                        <Switch checked={autoBackupEnabled} onChange={setAutoBackupEnabled} />
                      </div>
                      {autoBackupEnabled && (
                        <CronPresetSelect
                          value={autoBackupCron}
                          onChange={(val) => setAutoBackupCron(val)}
                        />
                      )}
                      {autoBackupEnabled && (
                        <Cron
                          value={cronTo5(autoBackupCron)}
                          setValue={(val: string) => {
                            setAutoBackupCron(cronTo6(val));
                          }}
                          locale={CRON_ZH_LOCALE}
                          defaultPeriod="day"
                          humanizeLabels
                          allowClear={false}
                        />
                      )}
                      {autoBackupEnabled && (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
                          <span style={{ fontSize: 12, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>保留数量</span>
                          <InputNumber
                            min={1}
                            max={1000}
                            value={autoBackupMaxFiles}
                            onChange={(v) => v && setAutoBackupMaxFiles(v)}
                            style={{ width: 80 }}
                            size="small"
                          />
                          <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>个备份文件</span>
                        </div>
                      )}
                      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
                        <Button size="small" type="primary" onClick={handleSaveAutoBackup} loading={backupLoading}>
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
                            <List.Item
                              style={{ padding: '6px 0', fontSize: 12 }}
                            >
                              <div>
                                <div style={{ fontWeight: 500 }}>{file.name}</div>
                                <div style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>
                                  {(file.size / 1024).toFixed(1)} KB · {file.created_at}
                                </div>
                              </div>
                              <Space size={4}>
                                <Button type="text" icon={<DownloadOutlined />} size="small" onClick={() => handleDownloadBackupFile(file.name)} />
                                <Popconfirm title="确定删除此备份？" onConfirm={() => handleDeleteBackup(file.name)}>
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
                      <InputNumber
                        min={1}
                        max={365}
                        value={logCleanupDays ?? undefined}
                        onChange={(v) => setLogCleanupDays(v ?? null)}
                        style={{ width: 80 }}
                        size="small"
                      />
                      <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>天</span>
                      <Button
                        size="small"
                        type="primary"
                        onClick={handleSaveLogCleanup}
                        style={{ marginLeft: 8 }}
                      >
                        保存
                      </Button>
                      <Button
                        size="small"
                        onClick={handleTriggerLogCleanup}
                        style={{ marginLeft: 4 }}
                      >
                        立即清理
                      </Button>
                    </div>
                  </div>
                </Card>
              </div>
            ),
          },
        ]}
      />

      {/* Import wizard modal */}
      <Modal
        title="导入预览"
        open={wizardOpen}
        onCancel={() => setWizardOpen(false)}
        onOk={handleWizardConfirm}
        okText={`导入 ${selectedRowKeys.length} 项`}
        cancelText="取消"
        confirmLoading={importing}
        width={800}
        okButtonProps={{ disabled: selectedRowKeys.length === 0 }}
      >
        <div style={{ marginBottom: 12, display: 'flex', gap: 16 }}>
          <AntTag color="green">{wizardItems.filter(i => i.action === 'new').length} 个新建</AntTag>
          <AntTag color="orange">{wizardItems.filter(i => i.action === 'overwrite').length} 个覆盖</AntTag>
          <AntTag color="blue">已选 {selectedRowKeys.length} 项</AntTag>
        </div>
        <Table
          dataSource={wizardItems}
          rowKey="key"
          size="small"
          pagination={false}
          scroll={{ y: 400 }}
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

      {/* Export select modal */}
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
    </div>
  );
}
