import { useState, useEffect, useCallback } from 'react';
import { Tabs, message } from 'antd';
import * as db from '@/utils/database';
import yaml from 'js-yaml';
import { TodoBackupTab } from './backup/TodoBackupTab';
import { SkillBackupTab } from './backup/SkillBackupTab';
import { DatabaseBackupTab } from './backup/DatabaseBackupTab';
import { LoopBackupTab } from '@/components/settings/backup/LoopBackupTab';
import { ImportExportModals, BackupDataYaml, ImportItem } from './backup/ImportExportModals';
import type { ProjectDirectory } from '@/utils/database/todos';

// 备份子 tab 的合法 key——每个子 tab 对应 URL 里的一个 sub 参数，支持深链直达
const BACKUP_SUB_KEYS = ['todo', 'skill-backup', 'loop', 'database'] as const;
type BackupSubKey = (typeof BACKUP_SUB_KEYS)[number];

// 从 hash URL（如 #/settings?tab=backup&sub=loop）解析当前备份子 tab
function getBackupSubFromHash(): BackupSubKey {
  const hash = window.location.hash;
  const qIdx = hash.indexOf('?');
  if (qIdx < 0) return 'todo';
  const sub = new URLSearchParams(hash.slice(qIdx + 1)).get('sub');
  // 只认合法 key，否则回退到「事项备份」
  return sub && (BACKUP_SUB_KEYS as readonly string[]).includes(sub) ? (sub as BackupSubKey) : 'todo';
}

// 把当前备份子 tab 写回 hash URL（replaceState 不污染浏览历史，仅更新深链）
function setBackupSubInHash(sub: BackupSubKey) {
  const hash = window.location.hash;
  const qIdx = hash.indexOf('?');
  const path = qIdx < 0 ? hash : hash.slice(0, qIdx);
  const params = new URLSearchParams(qIdx < 0 ? '' : hash.slice(qIdx + 1));
  params.set('sub', sub);
  window.history.replaceState(null, '', `${path}?${params.toString()}`);
}


export function BackupPanel() {
  // 备份子 tab：初始值来自 URL（深链），切换时同步回 URL
  const [backupSub, setBackupSub] = useState<BackupSubKey>(getBackupSubFromHash);
  // 浏览器前进/后退时按 URL 同步子 tab
  useEffect(() => {
    const onPop = () => setBackupSub(getBackupSubFromHash());
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  }, []);
  const handleBackupSubChange = useCallback((key: string) => {
    if (!(BACKUP_SUB_KEYS as readonly string[]).includes(key)) return;
    setBackupSub(key as BackupSubKey);
    setBackupSubInHash(key as BackupSubKey);
  }, []);

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
  const [wizardOpen, setWizardOpen] = useState(false);
  const [wizardItems, setWizardItems] = useState<ImportItem[]>([]);
  const [wizardTags, setWizardTags] = useState<{ name: string; color: string }[]>([]);
  const [selectedRowKeys, setSelectedRowKeys] = useState<number[]>([]);
  // 导入目标工作空间列表（父组件加载一次，下发给每行 WorkspaceSwitcher 复用）
  const [workspaces, setWorkspaces] = useState<ProjectDirectory[]>([]);
  // 逐行工作空间选择：ImportItem.key → workspaceId（null=未匹配/待指定）
  const [rowWorkspaceMap, setRowWorkspaceMap] = useState<Record<number, number | null>>({});
  // 当前库已有 todo：用于按「目标工作空间 + 标题 + prompt」动态判同名（对齐后端 merge_backup 判重口径）
  const [existingTodos, setExistingTodos] = useState<{ title: string; prompt: string; workspace_id: number | null }[]>([]);
  // 用户显式选「覆盖」的同名项 key（未在其中且同名的 = 默认跳过）
  const [userOverwriteKeys, setUserOverwriteKeys] = useState<Set<number>>(new Set());

  // 导入来源工作空间检测（从备份文件中提取，用于预览提示）
  const [sourceWorkspaceInfo, setSourceWorkspaceInfo] = useState<{ id: number; path: string } | null>(null);

  // 加载工作空间列表（不再选全局默认——逐行按各自原 id 匹配）
  const loadWorkspaces = async (): Promise<ProjectDirectory[]> => {
    try {
      const ws = await db.getProjectDirectories();
      setWorkspaces(ws);
      return ws;
    } catch (e) {
      console.error('Failed to load workspaces', e);
      return [];
    }
  };

  // 设置某行的目标工作空间（供 ImportExportModals 逐行回写）
  const setRowWorkspaceId = (key: number, id: number | null) => {
    setRowWorkspaceMap((prev) => ({ ...prev, [key]: id }));
  };

  // 按 wizardItems + 当前工作空间列表，初始化每行默认：原 id 命中→原 id，否则 null（未匹配）
  const buildDefaultRowWorkspace = (
    items: ImportItem[],
    ws: ProjectDirectory[],
  ): Record<number, number | null> => {
    const map: Record<number, number | null> = {};
    for (const it of items) {
      const matched = it.workspace_id != null && ws.some((w) => w.id === it.workspace_id);
      map[it.key] = matched ? (it.workspace_id as number) : null;
    }
    return map;
  };

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
      // 原生 fetch 下载二进制流，不经 axios 拦截器，手动写 v1 路径
      const response = await fetch('/api/v1/backup/database/download');
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
      // 原生 fetch 导出 YAML，不经 axios 拦截器，手动写 v1 路径
      const response = await fetch('/api/v1/backup/export', {
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

      // v1 纯 workspace-scoped：getAllTodos 需逐空间拉取后合并，
      // 用于备份导入去重时跨空间检查同名 todo
      const allExisting = await Promise.all(
        workspaces.map(ws => db.getAllTodos(ws.id).catch(() => []))
      );
      const existingTodos = allExisting.flat();
      setExistingTodos(existingTodos.map((t) => ({ title: t.title, prompt: t.prompt, workspace_id: t.workspace_id ?? null })));
      // 用户覆盖选择按导入批次重置
      setUserOverwriteKeys(new Set());

      // exists/action 不在此时静态判定：它们依赖用户逐行选定的目标工作空间，
      // 改由下方的 itemsWithAction 动态派生（对齐后端 merge_backup 的判重口径）。
      const items: ImportItem[] = data.todos.map((todo, idx) => ({
        key: idx,
        title: todo.title,
        prompt: todo.prompt,
        status: todo.status,
        executor: todo.executor,
        scheduler_enabled: todo.scheduler_enabled,
        scheduler_config: todo.scheduler_config,
        tag_names: todo.tag_names || [],
        workspace_path: todo.workspace_path,
        // 保留原始 workspace_id，供按行默认匹配与「来源」列展示
        workspace_id: todo.workspace_id ?? null,
        action: 'new',
      }));

      setWizardTags(data.tags || []);
      setWizardItems(items);
      setSelectedRowKeys(items.map(i => i.key));

      // 加载工作空间列表，并按行初始化默认归属（原 id 命中→原 id，否则 null 待指定）
      const ws = await loadWorkspaces();
      setRowWorkspaceMap(buildDefaultRowWorkspace(items, ws));

      // 从备份文件提取原始工作空间信息，仅作预览提示（逐行匹配，不再自动选全局）
      const firstTodoWithWs = data.todos.find((t) => t.workspace_id != null);
      if (firstTodoWithWs?.workspace_id != null) {
        setSourceWorkspaceInfo({
          id: Number(firstTodoWithWs.workspace_id),
          path: firstTodoWithWs.workspace_path || '',
        });
      } else {
        setSourceWorkspaceInfo(null);
      }
      setWizardOpen(true);
    } catch (err: any) {
      message.error('解析文件失败: ' + (err?.message || String(err)));
    }
    return false;
  };

  // 按「目标工作空间 + 标题 + prompt」动态派生每条的 exists/action，对齐后端 merge_backup：
  // 同一目标工作空间内 title+prompt 命中 → 同名（默认跳过，userOverwriteKeys 里的为覆盖）；否则新建。
  // 这样用户改某行工作空间后，该行是否同名会随之重算，避免跨工作空间误判同名导致「覆盖」变新建。
  const itemsWithAction: ImportItem[] = wizardItems.map((it) => {
    const targetWsId = rowWorkspaceMap[it.key] ?? 0; // null → 0（未分配哨兵，与后端一致）
    const exists = existingTodos.some(
      (t) => t.title === it.title && t.prompt === it.prompt && (t.workspace_id ?? 0) === targetWsId,
    );
    const action = exists ? (userOverwriteKeys.has(it.key) ? 'overwrite' : 'skip') : 'new';
    return { ...it, exists, action };
  });

  // 同名项动作：选「覆盖」记入 userOverwriteKeys，选「跳过」移出（默认即跳过）
  const setItemsAction = (keys: number[], action: 'overwrite' | 'skip') => {
    setUserOverwriteKeys((prev) => {
      const next = new Set(prev);
      for (const k of keys) {
        if (action === 'overwrite') next.add(k); else next.delete(k);
      }
      return next;
    });
  };

  const handleWizardConfirm = async () => {
    // 实际将导入：已勾选 且 action 非 skip（skip 不提交）；action 取动态派生值
    const willImport = itemsWithAction.filter(
      (item) => selectedRowKeys.includes(item.key) && item.action !== 'skip',
    );
    if (willImport.length === 0) {
      message.warning('没有可导入的项（选中的均为「跳过」）');
      return;
    }
    setImporting(true);
    try {
      // 每条 todo 注入用户逐行选定的工作空间；全局 target 传 null，由后端按每条 workspace_id 解析
      const selectedTodos = willImport.map(({ key, action, existingTitle, exists, ...todo }) => ({
        ...todo,
        workspace_id: rowWorkspaceMap[key] ?? null,
      }));
      const msg = await db.mergeBackup(wizardTags, selectedTodos, null);
      message.success(msg);
      setWizardOpen(false);
      window.location.reload();
    } catch (err: any) {
      message.error(err?.message || '导入失败');
    } finally {
      setImporting(false);
    }
  };

  return (
    <div>
      <Tabs
        activeKey={backupSub}
        onChange={handleBackupSubChange}
        items={[
          {
            key: 'todo',
            label: '事项备份',
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
        wizardItems={itemsWithAction}
        // 同名项动作批量/逐条设置
        setItemsAction={setItemsAction}
        // 逐行工作空间选择
        workspaces={workspaces}
        rowWorkspaceMap={rowWorkspaceMap}
        setRowWorkspaceId={setRowWorkspaceId}
        // 原始工作空间提示
        sourceWorkspaceInfo={sourceWorkspaceInfo}
      />
    </div>
  );
}
