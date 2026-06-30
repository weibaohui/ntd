import { useState, useEffect } from 'react';
import { Tabs, message } from 'antd';
import { useApp } from '@/hooks/useApp';
import * as db from '@/utils/database';
import yaml from 'js-yaml';
import { TodoBackupTab } from './backup/TodoBackupTab';
import { SkillBackupTab } from './backup/SkillBackupTab';
import { DatabaseBackupTab } from './backup/DatabaseBackupTab';
import { LoopBackupTab } from '@/components/settings/backup/LoopBackupTab';
import { ImportExportModals, BackupDataYaml, ImportItem } from './backup/ImportExportModals';

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
      const response = await fetch('/api/backup/database/download');
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
      const response = await fetch('/api/backup/export', {
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
          workspace_path: todo.workspace_path,
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
      const response = await fetch('/api/backup/export-selected', {
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
              <TodoBackupTab
                todoBackupStatus={todoBackupStatus}
                autoTodoBackupEnabled={autoTodoBackupEnabled}
                autoTodoBackupCron={autoTodoBackupCron}
                autoTodoBackupMaxFiles={autoTodoBackupMaxFiles}
                todoBackupLoading={todoBackupLoading}
                setAutoTodoBackupEnabled={setAutoTodoBackupEnabled}
                setAutoTodoBackupCron={setAutoTodoBackupCron}
                setAutoTodoBackupMaxFiles={setAutoTodoBackupMaxFiles}
                onTriggerBackup={handleTriggerTodoBackup}
                onSaveAutoBackup={handleSaveTodoAutoBackup}
                onDeleteBackup={handleDeleteTodoBackup}
                onDownloadBackupFile={handleDownloadTodoBackupFile}
                onExportBackup={handleExportBackup}
                onOpenExportModal={() => setExportModalOpen(true)}
                onImportFile={handleImportFile}
              />
            ),
          },
          {
            key: 'skill-backup',
            label: 'Skill备份',
            children: (
              <SkillBackupTab
                skillBackupStatus={skillBackupStatus}
                autoSkillBackupEnabled={autoSkillBackupEnabled}
                autoSkillBackupCron={autoSkillBackupCron}
                autoSkillBackupMaxFiles={autoSkillBackupMaxFiles}
                skillBackupLoading={skillBackupLoading}
                setAutoSkillBackupEnabled={setAutoSkillBackupEnabled}
                setAutoSkillBackupCron={setAutoSkillBackupCron}
                setAutoSkillBackupMaxFiles={setAutoSkillBackupMaxFiles}
                onTriggerBackup={handleTriggerSkillBackup}
                onSaveAutoBackup={handleSaveSkillAutoBackup}
                onDeleteBackup={handleDeleteSkillBackup}
                onDownloadBackupFile={handleDownloadSkillBackupFile}
              />
            ),
          },
          {
            key: 'loop',
            label: '环路备份',
            children: (
              <LoopBackupTab />
            ),
          },
          {
            key: 'database',
            label: '数据库备份',
            children: (
              <DatabaseBackupTab
                backupStatus={backupStatus}
                autoBackupEnabled={autoBackupEnabled}
                autoBackupCron={autoBackupCron}
                autoBackupMaxFiles={autoBackupMaxFiles}
                backupLoading={backupLoading}
                logCleanupDays={logCleanupDays}
                setAutoBackupEnabled={setAutoBackupEnabled}
                setAutoBackupCron={setAutoBackupCron}
                setAutoBackupMaxFiles={setAutoBackupMaxFiles}
                setLogCleanupDays={setLogCleanupDays}
                onTriggerBackup={handleTriggerBackup}
                onSaveAutoBackup={handleSaveAutoBackup}
                onDeleteBackup={handleDeleteBackup}
                onDownloadBackupFile={handleDownloadBackupFile}
                onDownloadDatabase={handleDownloadDatabase}
                onOptimizeDatabase={handleOptimizeDatabase}
                onSaveLogCleanup={handleSaveLogCleanup}
                onTriggerLogCleanup={handleTriggerLogCleanup}
              />
            ),
          },
        ]}
      />

      <ImportExportModals
        wizardOpen={wizardOpen}
        setWizardOpen={setWizardOpen}
        handleWizardConfirm={handleWizardConfirm}
        importing={importing}
        selectedRowKeys={selectedRowKeys}
        setSelectedRowKeys={setSelectedRowKeys}
        wizardItems={wizardItems}
        exportModalOpen={exportModalOpen}
        setExportModalOpen={setExportModalOpen}
        handleExportSelected={handleExportSelected}
        exportingSelected={exportingSelected}
        exportTodoKeys={exportTodoKeys}
        setExportTodoKeys={setExportTodoKeys}
        todos={todos}
      />
    </div>
  );
}
