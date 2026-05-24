import { useState, useEffect, useMemo } from 'react';
import {
  Tabs,
  Form,
  Input,
  InputNumber,
  Select,
  AutoComplete,
  Button,
  message,
  List,
  Popconfirm,
  ColorPicker,
  Upload,
  Empty,
  Card,
  Space,
  Typography,
  Spin,
  Modal,
  Table,
  Tag as AntTag,
  Switch,
  Tooltip,

} from 'antd';
import {
  SettingOutlined,
  CodeOutlined,
  TagOutlined,
  SaveOutlined,
  DownloadOutlined,
  DeleteOutlined,
  InboxOutlined,
  DatabaseOutlined,
  ClockCircleOutlined,
  ThunderboltOutlined,
  InfoCircleOutlined,
  MessageOutlined,
  QrcodeOutlined,
  CopyOutlined,
  ReloadOutlined,
  PlusOutlined,
  HistoryOutlined,
  QuestionCircleOutlined,
  MinusCircleOutlined,
  PlayCircleOutlined,
  StopOutlined,
  SearchOutlined,
  LaptopOutlined,
  FolderOutlined,
  EditOutlined,
  FileTextOutlined,
  LeftOutlined,
  ApiOutlined,
} from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import QRCode from 'qrcode';
import 'react-js-cron/dist/styles.css';
import { useApp } from '../hooks/useApp';
import * as db from '../utils/database';
import type { FeishuPushStatus, WhitelistEntry, ProjectDirectory } from '../utils/database';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '../utils/cron';
import type { Config, ExecutorConfig, FeishuHistoryMessage, FeishuHistoryChat, SlashCommandRule, ExecutionRecord, TodoTemplate, CustomTemplateStatus } from '../types';
import yaml from 'js-yaml';
import { CronPresetSelect } from './CronPresetSelect';
import { SkillsPanel } from './SkillsPanel';
import { SessionManager } from './SessionManager';
import { ShareCard } from './ShareCard';
import { WebhooksPanel } from './WebhooksPanel';

const { Paragraph } = Typography;
const { Dragger } = Upload;
const { Option } = Select;

const LOG_LEVELS = ['DEBUG', 'INFO', 'WARN', 'ERROR'];

// 常用时区列表
const TIMEZONES = [
  { value: 'UTC', label: 'UTC (世界协调时间)' },
  { value: 'Asia/Shanghai', label: 'Asia/Shanghai (北京时间, UTC+8)' },
  { value: 'Asia/Tokyo', label: 'Asia/Tokyo (东京时间, UTC+9)' },
  { value: 'Asia/Seoul', label: 'Asia/Seoul (首尔时间, UTC+9)' },
  { value: 'Asia/Singapore', label: 'Asia/Singapore (新加坡时间, UTC+8)' },
  { value: 'Asia/Hong_Kong', label: 'Asia/Hong_Kong (香港时间, UTC+8)' },
  { value: 'Asia/Dubai', label: 'Asia/Dubai (迪拜时间, UTC+4)' },
  { value: 'Europe/London', label: 'Europe/London (伦敦时间, UTC+0)' },
  { value: 'Europe/Paris', label: 'Europe/Paris (巴黎时间, UTC+1)' },
  { value: 'Europe/Berlin', label: 'Europe/Berlin (柏林时间, UTC+1)' },
  { value: 'America/New_York', label: 'America/New_York (纽约时间, UTC-5)' },
  { value: 'America/Los_Angeles', label: 'America/Los_Angeles (洛杉矶时间, UTC-8)' },
  { value: 'America/Chicago', label: 'America/Chicago (芝加哥时间, UTC-6)' },
  { value: 'America/Sao_Paulo', label: 'America/Sao_Paulo (圣保罗时间, UTC-3)' },
  { value: 'Australia/Sydney', label: 'Australia/Sydney (悉尼时间, UTC+10)' },
];

interface SettingsPageProps {
  onBack?: () => void;
}

export function SettingsPage({ onBack }: SettingsPageProps) {
  const { state, dispatch } = useApp();
  const { tags, todos } = state;

  const [configForm] = Form.useForm();
  const [configLoading, setConfigLoading] = useState(false);
  const [configSaving, setConfigSaving] = useState(false);

  const [tagName, setTagName] = useState('');
  const [tagColor, setTagColor] = useState('#0891b2');
  const [tagCreating, setTagCreating] = useState(false);

  // Project directories state
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  const [projectDirsLoading, setProjectDirsLoading] = useState(false);
  const [newDirPath, setNewDirPath] = useState('');
  const [newDirName, setNewDirName] = useState('');
  const [addingDir, setAddingDir] = useState(false);
  const [editingDirId, setEditingDirId] = useState<number | null>(null);
  const [editingDirName, setEditingDirName] = useState('');

  const [importing, setImporting] = useState(false);

  // Runtime management state
  const [selectedRecordIds, setSelectedRecordIds] = useState<number[]>([]);
  const [stoppingRecords, setStoppingRecords] = useState(false);
  const [runningRecords, setRunningRecords] = useState<ExecutionRecord[]>([]);
  // Execution record detail modal state
  const [execDetailRecord, setExecDetailRecord] = useState<ExecutionRecord | null>(null);
  // Selective export state
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportTodoKeys, setExportTodoKeys] = useState<number[]>([]);
  const [exportingSelected, setExportingSelected] = useState(false);

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

  // Log cleanup state
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

  // Version info state
  const [versionInfo, setVersionInfo] = useState<{ version: string; git_sha: string; git_describe: string } | null>(null);
  const [versionLoading, setVersionLoading] = useState(false);

  // Todo templates state
  const [templates, setTemplates] = useState<TodoTemplate[]>([]);
  const [templatesLoading, setTemplatesLoading] = useState(false);
  const [templateEditing, setTemplateEditing] = useState<TodoTemplate | null>(null);
  const [templateFormOpen, setTemplateFormOpen] = useState(false);
  const [templateFormTitle, setTemplateFormTitle] = useState('');
  const [templateFormPrompt, setTemplateFormPrompt] = useState('');
  const [templateFormCategory, setTemplateFormCategory] = useState('');
  const [templateFormSaving, setTemplateFormSaving] = useState(false);

  // Custom template state (remote URL subscription)
  const [customTemplateStatus, setCustomTemplateStatus] = useState<CustomTemplateStatus | null>(null);
  const [customTemplateLoading, setCustomTemplateLoading] = useState(false);
  const [customTemplateSubscribing, setCustomTemplateSubscribing] = useState(false);
  const [customTemplateUrl, setCustomTemplateUrl] = useState('');
  const [customTemplateAutoSyncEnabled, setCustomTemplateAutoSyncEnabled] = useState(false);
  const [customTemplateAutoSyncCron, setCustomTemplateAutoSyncCron] = useState('0 0 4 * * *');

  // Agent Bots state
  const [agentBots, setAgentBots] = useState<db.AgentBot[]>([]);

  // Executors state
  const [executors, setExecutors] = useState<ExecutorConfig[]>([]);
  const [executorsLoading, setExecutorsLoading] = useState(false);

  // Executor name -> display_name map derived from loaded executors
  const executorDisplayNames = useMemo(() => {
    const map: Record<string, string> = {};
    for (const ec of executors) {
      map[ec.name] = ec.display_name;
    }
    return map;
  }, [executors]);
  const [detectingExecutor, setDetectingExecutor] = useState<string | null>(null);
  const [testingExecutor, setTestingExecutor] = useState<string | null>(null);
  const [batchDetecting, setBatchDetecting] = useState(false);
  const [detectResults, setDetectResults] = useState<Record<string, { found: boolean; resolved: string | null }>>({});
  const [testModalVisible, setTestModalVisible] = useState(false);
  const [testModalData, setTestModalData] = useState<{ name: string; result: { test_passed: boolean; output: string | null; error: string | null } } | null>(null);
  const [savingExecutor, setSavingExecutor] = useState<string | null>(null);
  const [botsLoading, setBotsLoading] = useState(false);
  const [feishuPushStatus, setFeishuPushStatus] = useState<FeishuPushStatus[]>([]);
  const [groupWhitelist, setGroupWhitelist] = useState<WhitelistEntry[]>([]);
  const [whitelistOpenId, setWhitelistOpenId] = useState('');
  const [whitelistName, setWhitelistName] = useState('');
  const [whitelistBotId, setWhitelistBotId] = useState<number | null>(null);
  const [binding, setBinding] = useState(false);
  const [bindModalOpen, setBindModalOpen] = useState(false);
  const [qrCodeUrl, setQrCodeUrl] = useState('');
  const [pollError, setPollError] = useState('');
  const [bindSuccess, setBindSuccess] = useState(false);

  // Feishu history state
  const [historyMessages, setHistoryMessages] = useState<FeishuHistoryMessage[]>([]);
  const [historyChats, setHistoryChats] = useState<FeishuHistoryChat[]>([]);
  const [historySenders, setHistorySenders] = useState<db.FeishuSenderItem[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyTotal, setHistoryTotal] = useState(0);
  const [historyPage, setHistoryPage] = useState(1);
  const [historyPageSize, setHistoryPageSize] = useState(20);
  const [historySelectedChatId, setHistorySelectedChatId] = useState<string | undefined>(undefined);
  const [historyIsHistory, setHistoryIsHistory] = useState<boolean | undefined>(undefined);
  const [historySelectedSenderId, setHistorySelectedSenderId] = useState<string | undefined>(undefined);
  const [historyViewMsg, setHistoryViewMsg] = useState<string | null>(null);
  const [historyAddModalOpen, setHistoryAddModalOpen] = useState(false);
  const [historyForm] = Form.useForm();

  // Import wizard state
  const [wizardOpen, setWizardOpen] = useState(false);
  const [wizardItems, setWizardItems] = useState<ImportItem[]>([]);
  const [wizardTags, setWizardTags] = useState<{ name: string; color: string }[]>([]);
  const [selectedRowKeys, setSelectedRowKeys] = useState<number[]>([]);

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

  // Load config on mount
  useEffect(() => {
    setConfigLoading(true);
    db.getConfig()
      .then((cfg) => {
        configForm.setFieldsValue({
          ...cfg,
          slash_command_rules: cfg.slash_command_rules || [],
        });
      })
      .catch((err) => {
        message.error('加载配置失败: ' + (err?.message || String(err)));
      })
      .finally(() => setConfigLoading(false));
  }, [configForm]);

  // Load executors from database
  useEffect(() => {
    setExecutorsLoading(true);
    db.getExecutors()
      .then((list) => {
        setExecutors(list);
      })
      .catch((err) => {
        message.error('加载执行器配置失败: ' + (err?.message || String(err)));
      })
      .finally(() => setExecutorsLoading(false));
  }, []);

  // Load database backup status
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

  // Load Todo backup status
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

  // Load Skill backup status
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

  // Load version info
  useEffect(() => {
    setVersionLoading(true);
    db.getVersion()
      .then((info) => {
        setVersionInfo(info);
      })
      .catch(() => {})
      .finally(() => setVersionLoading(false));
  }, []);

  // Load todo templates
  useEffect(() => {
    setTemplatesLoading(true);
    db.getTodoTemplates()
      .then((list) => {
        setTemplates(list);
      })
      .catch((err) => {
        message.error('加载模板失败: ' + (err?.message || String(err)));
      })
      .finally(() => setTemplatesLoading(false));
  }, []);

  // Load custom template status
  const loadCustomTemplateStatus = () => {
    setCustomTemplateLoading(true);
    db.getCustomTemplateStatus()
      .then((status) => {
        setCustomTemplateStatus(status);
        setCustomTemplateAutoSyncEnabled(status.auto_sync_enabled);
        setCustomTemplateAutoSyncCron(status.auto_sync_cron);
      })
      .catch((err) => {
        console.error('加载自定义模板状态失败:', err);
      })
      .finally(() => setCustomTemplateLoading(false));
  };

  useEffect(() => {
    loadCustomTemplateStatus();
  }, []);

  // Load agent bots
  const loadAgentBots = () => {
    setBotsLoading(true);
    db.getAgentBots()
      .then((bots) => setAgentBots(bots))
      .catch(() => {})
      .finally(() => setBotsLoading(false));
  };

  const loadFeishuPush = () => {
    db.getFeishuPush()
      .then((status) => setFeishuPushStatus(status))
      .catch(() => {});
  };

  const loadGroupWhitelist = (botId: number) => {
    setWhitelistBotId(botId);
    db.getGroupWhitelist(botId)
      .then(setGroupWhitelist)
      .catch(() => setGroupWhitelist([]));
  };

  const handleAddWhitelist = async () => {
    if (!whitelistBotId || !whitelistOpenId.trim()) return;
    try {
      await db.addGroupWhitelist(whitelistBotId, whitelistOpenId.trim(), whitelistName.trim() || undefined);
      loadGroupWhitelist(whitelistBotId);
      setWhitelistOpenId('');
      setWhitelistName('');
    } catch (e: any) {
      message.error('添加白名单失败: ' + (e.message || '未知错误'));
    }
  };

  const handleDeleteWhitelist = async (id: number) => {
    if (!whitelistBotId) return;
    try {
      await db.deleteGroupWhitelist(id);
      loadGroupWhitelist(whitelistBotId);
    } catch (e: any) {
      message.error('删除白名单失败: ' + (e.message || '未知错误'));
    }
  };

  const loadHistoryMessages = async () => {
    setHistoryLoading(true);
    try {
      const data = await db.getFeishuHistoryMessages({
        chat_id: historySelectedChatId,
        is_history: historyIsHistory,
        sender_open_id: historySelectedSenderId,
        page: historyPage,
        page_size: historyPageSize,
      });
      setHistoryMessages(data.messages);
      setHistoryTotal(data.total);
    } catch {
      message.error('加载历史消息失败');
    } finally {
      setHistoryLoading(false);
    }
  };

  const loadHistoryChats = async () => {
    try {
      const data = await db.getFeishuHistoryChats();
      setHistoryChats(data);
    } catch (e) {
      console.error('加载群聊配置失败', e);
    }
  };

  const loadHistorySenders = async () => {
    try {
      const data = await db.getFeishuSenders();
      setHistorySenders(data);
    } catch (e) {
      console.error('加载发送者列表失败', e);
    }
  };

  useEffect(() => {
    loadHistoryChats();
    loadHistorySenders();
  }, []);

  useEffect(() => {
    loadHistoryMessages();
  }, [historyPage, historyPageSize, historySelectedChatId, historyIsHistory, historySelectedSenderId]);

  const handleAddHistoryChat = async () => {
    try {
      const values = await historyForm.validateFields();
      await db.createFeishuHistoryChat(values);
      message.success('添加成功');
      setHistoryAddModalOpen(false);
      historyForm.resetFields();
      loadHistoryChats();
    } catch (e) {
      if (e instanceof Error) {
        message.error(e.message);
      }
    }
  };

  useEffect(() => {
    loadAgentBots();
    loadFeishuPush();
  }, []);

  // 飞书绑定 — 后端轮询模式，前端只需一次调用
  const handleStartFeishuBind = async () => {
    setBinding(true);
    setBindSuccess(false);
    setPollError('');
    setQrCodeUrl('');
    setBindModalOpen(true);

    try {
      const initRes = await db.feishuInit();
      if (!initRes.supported) {
        setPollError('当前环境不支持 client_secret 认证');
        setBinding(false);
        return;
      }

      const beginRes = await db.feishuBegin();

      const qrDataUrl = await QRCode.toDataURL(beginRes.qr_url, {
        width: 256,
        margin: 2,
      });
      setQrCodeUrl(qrDataUrl);

      // 后端内部轮询，等待扫码结果
      const pollRes = await db.feishuPoll(beginRes.device_code, beginRes.interval, beginRes.expire_in);

      if (pollRes.success) {
        setBindSuccess(true);
        message.success(`绑定成功！Bot: ${pollRes.bot_name || 'Feishu Bot'}`);
        loadAgentBots();
        loadFeishuPush();
        setTimeout(() => {
          setBindModalOpen(false);
          setQrCodeUrl('');
        }, 2000);
      } else {
        const errMsg = pollRes.error === 'access_denied' ? '用户拒绝了绑定请求'
          : pollRes.error === 'expired_token' ? '二维码已过期，请重新绑定'
          : '绑定超时，请重试';
        setPollError(errMsg);
      }
    } catch (err: any) {
      setPollError(err?.message || '启动绑定失败');
    } finally {
      setBinding(false);
    }
  };

  const handleDeleteBot = async (botId: number) => {
    try {
      await db.deleteAgentBot(botId);
      message.success('已删除');
      loadAgentBots();
    } catch (err: any) {
      message.error(err?.message || '删除失败');
    }
  };

  const handleSaveConfig = async () => {
    try {
      const values = await configForm.validateFields();
      const currentConfig = await db.getConfig();
      const hasSlashRulesField = Object.prototype.hasOwnProperty.call(values, 'slash_command_rules');
      const slashRules = (((values as Config).slash_command_rules || []) as SlashCommandRule[])
        .map((rule) => ({
          slash_command: (rule.slash_command || '').trim(),
          todo_id: rule.todo_id,
          enabled: rule.enabled !== false,
        }))
        .filter((rule) => rule.slash_command || rule.todo_id);

      const normalizedRules = slashRules.map((rule) => ({
        ...rule,
        slash_command: rule.slash_command.startsWith('/') ? rule.slash_command : `/${rule.slash_command}`,
      }));

      const duplicateCommands = normalizedRules.reduce<string[]>((acc, rule, index) => {
        if (!rule.slash_command) return acc;
        const firstIndex = normalizedRules.findIndex((item) => item.slash_command === rule.slash_command);
        if (firstIndex !== index && !acc.includes(rule.slash_command)) {
          acc.push(rule.slash_command);
        }
        return acc;
      }, []);

      if (duplicateCommands.length > 0) {
        message.error(`存在重复命令: ${duplicateCommands.join('、')}`);
        return;
      }

      const mergedConfig: Config = {
        ...currentConfig,
        ...(values as Partial<Config>),
        max_concurrent_todos: configForm.getFieldValue('max_concurrent_todos') ?? currentConfig.max_concurrent_todos ?? 3,
        execution_timeout_secs: configForm.getFieldValue('execution_timeout_secs') ?? currentConfig.execution_timeout_secs ?? 1800,
        slash_command_rules: hasSlashRulesField
          ? normalizedRules
          : (currentConfig.slash_command_rules ?? []),
      };

      setConfigSaving(true);
      await db.updateConfig(mergedConfig);
      configForm.setFieldsValue(mergedConfig);
      message.success('配置已保存');
    } catch (err: any) {
      if (err?.errorFields) return; // validation error
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setConfigSaving(false);
    }
  };

  const handleCreateTag = async () => {
    if (tagCreating) return;
    const name = tagName.trim();
    if (!name) {
      message.error('请输入标签名称');
      return;
    }
    setTagCreating(true);
    try {
      const newTag = await db.createTag(name, tagColor);
      dispatch({ type: 'ADD_TAG', payload: newTag });
      message.success('标签创建成功');
      setTagName('');
      setTagColor('#0891b2');
    } catch (err: any) {
      message.error('创建失败: ' + (err?.message || String(err)));
    } finally {
      setTagCreating(false);
    }
  };

  const handleDeleteTag = async (tagId: number) => {
    try {
      await db.deleteTag(tagId);
      dispatch({ type: 'DELETE_TAG', payload: tagId });
      message.success('标签已删除');
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

  // Template management handlers
  const openTemplateForm = (template?: TodoTemplate) => {
    if (template) {
      setTemplateEditing(template);
      setTemplateFormTitle(template.title);
      setTemplateFormPrompt(template.prompt || '');
      setTemplateFormCategory(template.category);
    } else {
      setTemplateEditing(null);
      setTemplateFormTitle('');
      setTemplateFormPrompt('');
      setTemplateFormCategory('');
    }
    setTemplateFormOpen(true);
  };

  const closeTemplateForm = () => {
    setTemplateFormOpen(false);
    setTemplateEditing(null);
  };

  const handleSaveTemplate = async () => {
    const title = templateFormTitle.trim();
    const prompt = templateFormPrompt.trim();
    const category = templateFormCategory.trim();
    if (!title) {
      message.error('请输入模板标题');
      return;
    }
    if (!category) {
      message.error('请输入模板分类');
      return;
    }
    setTemplateFormSaving(true);
    try {
      if (templateEditing) {
        await db.updateTodoTemplate(templateEditing.id, title, prompt || null, category);
        setTemplates(prev => prev.map(t => t.id === templateEditing.id ? { ...t, title, prompt: prompt || null, category } : t));
        message.success('模板已更新');
      } else {
        const newTemplate = await db.createTodoTemplate(title, prompt || null, category);
        setTemplates(prev => [...prev, newTemplate]);
        message.success('模板已创建');
      }
      closeTemplateForm();
    } catch (err: any) {
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setTemplateFormSaving(false);
    }
  };

  const handleDeleteTemplate = async (templateId: number) => {
    try {
      await db.deleteTodoTemplate(templateId);
      setTemplates(prev => prev.filter(t => t.id !== templateId));
      message.success('模板已删除');
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

  const handleCopyTemplate = async (templateId: number) => {
    try {
      const newTemplate = await db.copyTodoTemplate(templateId);
      setTemplates(prev => [...prev, newTemplate]);
      message.success('模板已复制');
    } catch (err: any) {
      message.error('复制失败: ' + (err?.message || String(err)));
    }
  };

  // Custom template handlers
  const handleSubscribeCustomTemplate = async () => {
    if (!customTemplateUrl.trim()) {
      message.error('请输入模板地址');
      return;
    }
    setCustomTemplateSubscribing(true);
    try {
      const status = await db.subscribeCustomTemplate(customTemplateUrl.trim());
      setCustomTemplateStatus(status);
      // Reload templates to include new custom templates
      const list = await db.getTodoTemplates();
      setTemplates(list);
      message.success('订阅成功');
    } catch (err: any) {
      message.error('订阅失败: ' + (err?.message || String(err)));
    } finally {
      setCustomTemplateSubscribing(false);
    }
  };

  const handleUnsubscribeCustomTemplate = async () => {
    try {
      await db.unsubscribeCustomTemplate();
      setCustomTemplateStatus(null);
      setCustomTemplateUrl('');
      // Reload templates to remove custom templates
      const list = await db.getTodoTemplates();
      setTemplates(list);
      message.success('已取消订阅');
    } catch (err: any) {
      message.error('取消订阅失败: ' + (err?.message || String(err)));
    }
  };

  const handleSyncCustomTemplate = async () => {
    setCustomTemplateLoading(true);
    try {
      const status = await db.syncCustomTemplate();
      setCustomTemplateStatus(status);
      // Reload templates to include updated custom templates
      const list = await db.getTodoTemplates();
      setTemplates(list);
      message.success('同步成功');
    } catch (err: any) {
      message.error('同步失败: ' + (err?.message || String(err)));
    } finally {
      setCustomTemplateLoading(false);
    }
  };

  const handleUpdateCustomTemplateAutoSync = async () => {
    try {
      await db.updateCustomTemplateAutoSync(customTemplateAutoSyncEnabled, customTemplateAutoSyncCron);
      message.success('自动同步配置已更新');
    } catch (err: any) {
      message.error('更新失败: ' + (err?.message || String(err)));
    }
  };

  // Project directories handlers
  const loadProjectDirectories = () => {
    setProjectDirsLoading(true);
    db.getProjectDirectories()
      .then(setProjectDirectories)
      .catch((err: any) => message.error('加载项目目录失败: ' + (err?.message || String(err))))
      .finally(() => setProjectDirsLoading(false));
  };

  useEffect(() => {
    loadProjectDirectories();
  }, []);

  const handleAddProjectDirectory = async () => {
    const path = newDirPath.trim();
    if (!path) {
      message.error('请输入目录路径');
      return;
    }
    setAddingDir(true);
    try {
      const dir = await db.createProjectDirectory(path, newDirName.trim() || undefined);
      setProjectDirectories(prev => [...prev.filter(d => d.id !== dir.id), dir].sort((a, b) => a.path.localeCompare(b.path)));
      setNewDirPath('');
      setNewDirName('');
      message.success('添加成功');
    } catch (err: any) {
      message.error('添加失败: ' + (err?.message || String(err)));
    } finally {
      setAddingDir(false);
    }
  };

  const handleUpdateProjectDirectoryName = async (id: number) => {
    try {
      await db.updateProjectDirectory(id, editingDirName.trim() || undefined);
      setProjectDirectories(prev => prev.map(d => d.id === id ? { ...d, name: editingDirName.trim() || null } : d));
      setEditingDirId(null);
      setEditingDirName('');
      message.success('更新成功');
    } catch (err: any) {
      message.error('更新失败: ' + (err?.message || String(err)));
    }
  };

  const handleDeleteProjectDirectory = async (id: number) => {
    try {
      await db.deleteProjectDirectory(id);
      setProjectDirectories(prev => prev.filter(d => d.id !== id));
      message.success('删除成功');
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

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

      // 获取现有 todos 用于对比
      const existingTodos = await db.getAllTodos();
      const existingSet = new Set(existingTodos.map(t => `${t.title}\n${t.prompt}`));

      // 构建导入列表
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

  // Selective export
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

  // Database backup handlers
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

  // Load running execution records from DB
  const loadRunningRecords = async () => {
    try {
      const records = await db.getRunningExecutionRecords();
      setRunningRecords(records);
    } catch (err) {
      console.error('Failed to load running records:', err);
    }
  };

  useEffect(() => {
    loadRunningRecords();
    const timer = setInterval(loadRunningRecords, 10000);
    return () => clearInterval(timer);
  }, []);

  // Batch stop running records
  const handleBatchStop = async () => {
    if (selectedRecordIds.length === 0) return;
    setStoppingRecords(true);
    const results = await Promise.allSettled(
      selectedRecordIds.map(async (recordId) => {
        await db.forceFailExecution(recordId);
      })
    );
    const successCount = results.filter(r => r.status === 'fulfilled').length;
    const failCount = results.filter(r => r.status === 'rejected').length;
    setSelectedRecordIds([]);
    setStoppingRecords(false);
    if (successCount > 0) message.success(`已停止 ${successCount} 个任务`);
    if (failCount > 0) message.error(`${failCount} 个任务停止失败`);
    loadRunningRecords();
  };

  const tabItems = [
    {
      key: 'system',
      label: (
        <span>
          <SettingOutlined style={{ marginRight: 6 }} />
          系统设置
        </span>
      ),
      children: (
        <Spin spinning={configLoading}>
          <Form
            form={configForm}
            layout="vertical"
            style={{ maxWidth: 600 }}
            initialValues={{
              port: 8088,
              host: '0.0.0.0',
              db_path: '~/.ntd/data.db',
              log_level: 'INFO',
            }}
          >
            <Form.Item
              name="port"
              label="服务端口"
              rules={[{ required: true, type: 'integer', min: 1, max: 65535 }]}
            >
              <InputNumber style={{ width: '100%' }} placeholder="8088" />
            </Form.Item>
            <Form.Item
              name="host"
              label="服务地址"
              rules={[{ required: true }]}
            >
              <Input placeholder="0.0.0.0" />
            </Form.Item>
            <Form.Item
              name="db_path"
              label="数据库路径"
              rules={[{ required: true }]}
            >
              <Input placeholder="~/.ntd/data.db" />
            </Form.Item>
            <Form.Item
              name="log_level"
              label="日志级别"
              rules={[{ required: true }]}
            >
              <Select placeholder="选择日志级别">
                {LOG_LEVELS.map((level) => (
                  <Option key={level} value={level}>
                    {level}
                  </Option>
                ))}
              </Select>
            </Form.Item>
            <Form.Item
              name="scheduler_default_timezone"
              label="定时任务默认时区"
              tooltip="设置创建定时任务时的默认时区。例如：选择 Asia/Shanghai 后，每天 9:00 执行的任务会按北京时间 9:00 执行（而不是服务器本地时间）。"
            >
              <Select
                showSearch
                placeholder="选择默认时区"
                allowClear
                filterOption={(input, option) =>
                  (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
                }
                options={TIMEZONES}
              />
            </Form.Item>
            <Form.Item>
              <Button
                type="primary"
                onClick={handleSaveConfig}
                loading={configSaving}
                disabled={configLoading}
              >
                保存配置
              </Button>
            </Form.Item>
          </Form>
        </Spin>
      ),
    },
    {
      key: 'executors',
      label: (
        <span>
          <CodeOutlined style={{ marginRight: 6 }} />
          执行器管理
        </span>
      ),
      children: (
        <Spin spinning={executorsLoading}>
          <div style={{ maxWidth: 800 }}>
            <Paragraph type="secondary" style={{ marginBottom: 16 }}>
              管理执行器的路径、开关状态，并检测二进制是否可用。关闭开关的执行器不会出现在 Todo 的执行器选择列表中。
            </Paragraph>
            <div style={{ marginBottom: 12 }}>
              <Button
                type="primary"
                icon={<SearchOutlined />}
                loading={batchDetecting}
                onClick={async () => {
                  setBatchDetecting(true);
                  let availableCount = 0;
                  try {
                    for (const ec of executors) {
                      try {
                        const result = await db.detectExecutor(ec.name);
                        // Update detect results immediately
                        setDetectResults((prev) => ({
                          ...prev,
                          [ec.name]: { found: result.binary_found, resolved: result.path_resolved },
                        }));

                        // Update executor enabled state based on detection result
                        if (result.binary_found) {
                          availableCount++;
                          if (!ec.enabled) {
                            const updated = await db.updateExecutor(ec.name, { enabled: true });
                            setExecutors((prev) =>
                              prev.map((e) => (e.name === ec.name ? updated : e))
                            );
                          }
                        } else if (ec.enabled) {
                          const updated = await db.updateExecutor(ec.name, { enabled: false });
                          setExecutors((prev) =>
                            prev.map((e) => (e.name === ec.name ? updated : e))
                          );
                        }
                      } catch (err) {
                        // Continue with next executor on individual detection failure
                      }
                    }
                    message.success(`批量检测完成：${availableCount}/${executors.length} 个执行器可用`);
                  } catch (err: any) {
                    message.error('批量检测失败: ' + (err?.message || String(err)));
                  } finally {
                    setBatchDetecting(false);
                  }
                }}
              >
                批量检测
              </Button>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              {executors.map((ec) => {
                const detectResult = detectResults[ec.name];
                const isDetecting = detectingExecutor === ec.name;
                const isTesting = testingExecutor === ec.name;
                const isSaving = savingExecutor === ec.name;
                return (
                  <Card
                    key={ec.name}
                    size="small"
                    style={{
                      opacity: ec.enabled ? 1 : 0.6,
                      borderColor: ec.enabled ? undefined : '#d9d9d9',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
                      <Switch
                        checked={ec.enabled}
                        loading={isSaving}
                        onChange={async (checked) => {
                          setSavingExecutor(ec.name);
                          try {
                            const updated = await db.updateExecutor(ec.name, { enabled: checked });
                            setExecutors((prev) => prev.map((e) => e.name === ec.name ? updated : e));
                          } catch (err: any) {
                            message.error('更新失败: ' + (err?.message || String(err)));
                          } finally {
                            setSavingExecutor(null);
                          }
                        }}
                      />
                      <span style={{ fontWeight: 500, minWidth: 90 }}>{ec.display_name}</span>
                      <Input
                        style={{ flex: 1, minWidth: 200 }}
                        placeholder="二进制路径或命令名"
                        defaultValue={ec.path}
                        onBlur={async (e) => {
                          const newPath = e.target.value.trim();
                          if (newPath === ec.path) return;
                          setSavingExecutor(ec.name);
                          try {
                            const updated = await db.updateExecutor(ec.name, { path: newPath });
                            setExecutors((prev) => prev.map((ex) => ex.name === ec.name ? updated : ex));
                            setDetectResults((prev) => {
                              const next = { ...prev };
                              delete next[ec.name];
                              return next;
                            });
                          } catch (err: any) {
                            message.error('保存失败: ' + (err?.message || String(err)));
                          } finally {
                            setSavingExecutor(null);
                          }
                        }}
                        onPressEnter={(e) => {
                          (e.target as HTMLInputElement).blur();
                        }}
                      />
                      <Input
                        style={{ flex: 1, minWidth: 180 }}
                        placeholder="Session 目录（如 ~/.claude）"
                        defaultValue={ec.session_dir}
                        onBlur={async (e) => {
                          const newDir = e.target.value.trim();
                          if (newDir === ec.session_dir) return;
                          setSavingExecutor(ec.name);
                          try {
                            const updated = await db.updateExecutor(ec.name, { session_dir: newDir });
                            setExecutors((prev) => prev.map((ex) => ex.name === ec.name ? updated : ex));
                          } catch (err: any) {
                            message.error('保存失败: ' + (err?.message || String(err)));
                          } finally {
                            setSavingExecutor(null);
                          }
                        }}
                        onPressEnter={(e) => {
                          (e.target as HTMLInputElement).blur();
                        }}
                      />
                      <Button
                        size="small"
                        icon={<SearchOutlined />}
                        loading={isDetecting}
                        onClick={async () => {
                          setDetectingExecutor(ec.name);
                          try {
                            const result = await db.detectExecutor(ec.name);
                            setDetectResults((prev) => ({ ...prev, [ec.name]: { found: result.binary_found, resolved: result.path_resolved } }));
                            if (result.binary_found) {
                              message.success(`${ec.display_name}: 找到 (${result.path_resolved})`);
                            } else {
                              message.warning(`${ec.display_name}: 未找到`);
                            }
                          } catch (err: any) {
                            message.error('检测失败: ' + (err?.message || String(err)));
                          } finally {
                            setDetectingExecutor(null);
                          }
                        }}
                      >
                        检测
                      </Button>
                      <Button
                        size="small"
                        type="primary"
                        ghost
                        icon={<PlayCircleOutlined />}
                        loading={isTesting}
                        onClick={async () => {
                          setTestingExecutor(ec.name);
                          try {
                            const result = await db.testExecutor(ec.name);
                            setTestModalData({ name: ec.name, result });
                            setTestModalVisible(true);
                          } catch (err: any) {
                            message.error('测试失败: ' + (err?.message || String(err)));
                          } finally {
                            setTestingExecutor(null);
                          }
                        }}
                      >
                        测试
                      </Button>
                      {detectResult && (
                        <Tooltip title={detectResult.resolved || '未找到'}>
                          {detectResult.found
                            ? <span style={{ color: '#52c41a', fontSize: 16 }}>&#10003;</span>
                            : <span style={{ color: '#ff4d4f', fontSize: 16 }}>&#10007;</span>
                          }
                        </Tooltip>
                      )}
                    </div>
                  </Card>
                );
              })}
            </div>
          </div>
          <Modal
            title={testModalData ? `测试结果 - ${executors.find(e => e.name === testModalData.name)?.display_name || testModalData.name}` : '测试结果'}
            open={testModalVisible}
            onCancel={() => setTestModalVisible(false)}
            footer={<Button onClick={() => setTestModalVisible(false)}>关闭</Button>}
            width={500}
          >
            {testModalData && (
              <div>
                <p>
                  状态：{testModalData.result.test_passed
                    ? <span style={{ color: '#52c41a', fontWeight: 600 }}>通过</span>
                    : <span style={{ color: '#ff4d4f', fontWeight: 600 }}>失败</span>
                  }
                </p>
                {testModalData.result.error && (
                  <p style={{ color: '#ff4d4f' }}>错误：{testModalData.result.error}</p>
                )}
                {testModalData.result.output && (
                  <div>
                    <Paragraph type="secondary">输出：</Paragraph>
                    <pre style={{
                      background: '#f5f5f5',
                      padding: 12,
                      borderRadius: 6,
                      fontSize: 12,
                      maxHeight: 300,
                      overflow: 'auto',
                      whiteSpace: 'pre-wrap',
                    }}>
                      {testModalData.result.output}
                    </pre>
                  </div>
                )}
              </div>
            )}
          </Modal>
        </Spin>
      ),
    },
    {
      key: 'tags',
      label: (
        <span>
          <TagOutlined style={{ marginRight: 6 }} />
          标签管理
        </span>
      ),
      children: (
        <div style={{ maxWidth: 600 }}>
          <Card
            title="创建新标签"
            size="small"
            style={{ marginBottom: 24 }}
          >
            <Space direction="vertical" style={{ width: '100%' }}>
              <Input
                value={tagName}
                onChange={(e) => setTagName(e.target.value)}
                placeholder="输入标签名称"
                onPressEnter={handleCreateTag}
              />
              <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <ColorPicker
                  value={tagColor}
                  onChange={(_, hex) => setTagColor(hex)}
                  showText
                />
                <Button
                  type="primary"
                  loading={tagCreating}
                  onClick={handleCreateTag}
                >
                  创建标签
                </Button>
              </div>
            </Space>
          </Card>

          <div style={{ marginBottom: 12, fontWeight: 600 }}>现有标签</div>
          {tags.length === 0 ? (
            <Empty description="暂无标签" image={Empty.PRESENTED_IMAGE_SIMPLE} />
          ) : (
            <List
              dataSource={tags}
              renderItem={(tag) => (
                <List.Item
                  style={{
                    padding: '10px 12px',
                    background: 'var(--color-bg)',
                    borderRadius: 6,
                    marginBottom: 8,
                    border: '1px solid var(--color-border-light)',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1 }}>
                    <span
                      style={{
                        width: 16,
                        height: 16,
                        borderRadius: '50%',
                        backgroundColor: tag.color,
                        flexShrink: 0,
                      }}
                    />
                    <span style={{ fontSize: 14, fontWeight: 500 }}>{tag.name}</span>
                  </div>
                  <Popconfirm
                    title="删除标签"
                    description={`确定要删除标签 "${tag.name}" 吗？`}
                    onConfirm={() => handleDeleteTag(tag.id)}
                  >
                    <Button type="text" danger icon={<DeleteOutlined />} size="small" />
                  </Popconfirm>
                </List.Item>
              )}
            />
          )}
        </div>
      ),
    },
    {
      key: 'projectDirectories',
      label: (
        <span>
          <FolderOutlined style={{ marginRight: 6 }} />
          项目目录
        </span>
      ),
      children: (
        <div style={{ maxWidth: 700 }}>
          <Spin spinning={projectDirsLoading}>
            <Card title="添加项目目录" size="small" style={{ marginBottom: 24 }}>
              <Space direction="vertical" style={{ width: '100%' }}>
                <Paragraph type="secondary" style={{ fontSize: 13 }}>
                  添加常用项目目录，方便在为 Todo 选择执行目录时快速点选。目录路径必填，名称选填。
                </Paragraph>
                <div style={{ display: 'flex', gap: 8, alignItems: 'flex-start' }}>
                  <Input
                    value={newDirPath}
                    onChange={(e) => setNewDirPath(e.target.value)}
                    placeholder="目录路径（必填）"
                    style={{ flex: 2 }}
                    onPressEnter={handleAddProjectDirectory}
                  />
                  <Input
                    value={newDirName}
                    onChange={(e) => setNewDirName(e.target.value)}
                    placeholder="名称（选填）"
                    style={{ flex: 1 }}
                    onPressEnter={handleAddProjectDirectory}
                  />
                  <Button
                    type="primary"
                    icon={<PlusOutlined />}
                    loading={addingDir}
                    onClick={handleAddProjectDirectory}
                  >
                    添加
                  </Button>
                </div>
              </Space>
            </Card>

            <div style={{ marginBottom: 12, fontWeight: 600 }}>已添加的目录</div>
            {projectDirectories.length === 0 ? (
              <Empty description="暂无项目目录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
            ) : (
              <List
                dataSource={projectDirectories}
                renderItem={(dir) => (
                  <List.Item
                    style={{
                      padding: '12px',
                      background: 'var(--color-bg)',
                      borderRadius: 6,
                      marginBottom: 8,
                      border: '1px solid var(--color-border-light)',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1, minWidth: 0 }}>
                      <FolderOutlined style={{ fontSize: 18, color: '#1890ff', flexShrink: 0 }} />
                      <div style={{ flex: 1, minWidth: 0 }}>
                        {editingDirId === dir.id ? (
                          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                            <Input
                              value={editingDirName}
                              onChange={(e) => setEditingDirName(e.target.value)}
                              placeholder="输入名称"
                              size="small"
                              style={{ width: 120 }}
                              onPressEnter={() => handleUpdateProjectDirectoryName(dir.id)}
                              autoFocus
                            />
                            <Button size="small" type="primary" onClick={() => handleUpdateProjectDirectoryName(dir.id)}>保存</Button>
                            <Button size="small" onClick={() => { setEditingDirId(null); setEditingDirName(''); }}>取消</Button>
                          </div>
                        ) : (
                          <>
                            <div style={{ fontSize: 14, fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                              {dir.name || dir.path}
                            </div>
                            {dir.name && (
                              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                {dir.path}
                              </div>
                            )}
                          </>
                        )}
                      </div>
                    </div>
                    <Space size={4}>
                      {editingDirId !== dir.id && (
                        <Button
                          type="text"
                          icon={<EditOutlined />}
                          size="small"
                          onClick={() => { setEditingDirId(dir.id); setEditingDirName(dir.name || ''); }}
                        />
                      )}
                      <Popconfirm
                        title="删除目录"
                        description={`确定要删除 "${dir.name || dir.path}" 吗？`}
                        onConfirm={() => handleDeleteProjectDirectory(dir.id)}
                      >
                        <Button type="text" danger icon={<DeleteOutlined />} size="small" />
                      </Popconfirm>
                    </Space>
                  </List.Item>
                )}
              />
            )}
          </Spin>
        </div>
      ),
    },
    {
      key: 'backup',
      label: (
        <span>
          <SaveOutlined style={{ marginRight: 6 }} />
          备份与恢复
        </span>
      ),
      children: (
        <Tabs
          defaultActiveKey="todo"
          items={[
            {
              key: 'todo',
              label: 'Todo备份',
              children: (
                <div style={{ maxWidth: 600 }}>
                  <Card title="导出备份" size="small" style={{ marginBottom: 24 }}>
                    <Space direction="vertical" style={{ width: '100%' }}>
                      <Paragraph type="secondary">
                        将 Todo 和标签导出为 YAML 文件，方便迁移和存档
                      </Paragraph>
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
                    </Space>
                  </Card>

                  <Card title="导入备份" size="small" style={{ marginBottom: 24 }}>
                    <Space direction="vertical" style={{ width: '100%' }}>
                      <Paragraph type="secondary">
                        从 YAML 文件恢复数据，支持预览和选择性导入
                      </Paragraph>
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
                    </Space>
                  </Card>

                  <Card title="Todo自动备份" size="small">
                    <Space direction="vertical" style={{ width: '100%' }} size="middle">
                      <Paragraph type="secondary">
                        将 Todo 和标签打包备份到服务器，支持定时自动备份
                      </Paragraph>
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
                    </Space>
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
                    <Space direction="vertical" style={{ width: '100%' }} size="middle">
                      <Paragraph type="secondary">
                        备份各执行器下的 skills 文件夹，包含所有自定义和内置技能
                      </Paragraph>
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
                    </Space>
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
                    <Space direction="vertical" style={{ width: '100%' }} size="middle">
                      <Paragraph type="secondary">
                        直接备份 SQLite 数据库文件，包含所有数据（含执行记录）
                      </Paragraph>
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
                    </Space>
                  </Card>

                  <Card title="清理日志" size="small" style={{ marginTop: 16 }}>
                    <Space direction="vertical" style={{ width: '100%' }} size="middle">
                      <Paragraph type="secondary">
                        清理 execution_logs 表中早于指定天数的日志记录，释放数据库空间
                      </Paragraph>
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
                    </Space>
                  </Card>
                </div>
              ),
            },
          ]}
        />
      ),
    },
    {
      key: 'skills',
      label: (
        <span>
          <ThunderboltOutlined style={{ marginRight: 6 }} />
          Skills 管理
        </span>
      ),
      children: <SkillsPanel />,
    },
    {
      key: 'runtime',
      label: (
        <span>
          <PlayCircleOutlined style={{ marginRight: 6 }} />
          运行管理
        </span>
      ),
      children: (
        <div style={{ padding: '8px 0' }}>
          {/* 运行配置 */}
          <Card
            size="small"
            title="运行配置"
            style={{ marginBottom: 16 }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 24, flexWrap: 'wrap' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>最大并发数</span>
                <InputNumber
                  size="small"
                  min={1}
                  max={20}
                  value={configForm.getFieldValue('max_concurrent_todos') ?? 1}
                  onChange={(v) => {
                    if (v) {
                      configForm.setFieldsValue({ max_concurrent_todos: v });
                    }
                  }}
                  style={{ width: 70 }}
                />
                <Tooltip title="同时运行的最大 Todo 数量，超出将排队等待">
                  <InfoCircleOutlined style={{ color: 'var(--color-text-quaternary)', fontSize: 12 }} />
                </Tooltip>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>超时时间(分钟)</span>
                <InputNumber
                  size="small"
                  min={1}
                  max={1440}
                  style={{ width: 80 }}
                  value={Math.round((configForm.getFieldValue('execution_timeout_secs') ?? 1800) / 60)}
                  onChange={(v) => {
                    if (v) {
                      configForm.setFieldsValue({ execution_timeout_secs: v * 60 });
                    }
                  }}
                />
                <Tooltip title="单个对话执行的最大时长，超时将自动终止">
                  <InfoCircleOutlined style={{ color: 'var(--color-text-quaternary)', fontSize: 12 }} />
                </Tooltip>
              </div>
              <Button
                size="small"
                type="primary"
                icon={<SaveOutlined />}
                loading={configSaving}
                onClick={handleSaveConfig}
              >
                保存
              </Button>
            </div>
          </Card>

          {/* 运行中任务列表 */}
          <div style={{ marginBottom: 12, display: 'flex', alignItems: 'center', gap: 8 }}>
            <Button
              danger
              size="small"
              icon={<StopOutlined />}
              disabled={selectedRecordIds.length === 0}
              loading={stoppingRecords}
              onClick={handleBatchStop}
            >
              批量停止 ({selectedRecordIds.length})
            </Button>
            <Button
              size="small"
              icon={<ReloadOutlined />}
              onClick={loadRunningRecords}
            >
              刷新
            </Button>
            <span style={{ color: 'var(--color-text-secondary)', fontSize: 12 }}>
              共 {runningRecords.length} 个运行中任务
            </span>
          </div>
          <Table
            size="small"
            rowKey="id"
            dataSource={runningRecords}
            rowSelection={{
              selectedRowKeys: selectedRecordIds,
              onChange: (keys) => setSelectedRecordIds(keys as number[]),
            }}
            pagination={false}
            columns={[
              {
                title: 'Todo',
                key: 'todo_title',
                ellipsis: true,
                render: (_: unknown, record: ExecutionRecord) => {
                  const todo = todos.find(t => t.id === record.todo_id);
                  return todo ? todo.title : `#${record.todo_id}`;
                },
              },
              {
                title: '执行器',
                dataIndex: 'executor',
                key: 'executor',
                width: 110,
                render: (v: string | null) => {
                  return executorDisplayNames[v || ''] || v || '-';
                },
              },
              {
                title: '触发方式',
                dataIndex: 'trigger_type',
                key: 'trigger_type',
                width: 100,
                render: (v: string) => {
                  const map: Record<string, string> = { manual: '手动', slash_command: '斜杠命令', default_response: '默认响应', scheduler: '定时' };
                  return map[v] || v;
                },
              },
              {
                title: '开始时间',
                dataIndex: 'started_at',
                key: 'started_at',
                width: 170,
                render: (v: string) => v ? new Date(v).toLocaleString() : '-',
              },
              {
                title: '操作',
                key: 'action',
                width: 80,
                render: (_: unknown, record: ExecutionRecord) => (
                  <Popconfirm title="确认停止此任务？" onConfirm={async () => {
                    try {
                      await db.forceFailExecution(record.id);
                      message.success('已停止');
                      loadRunningRecords();
                    } catch (err) { message.error(`停止失败: ${err instanceof Error ? err.message : String(err)}`); }
                  }}>
                    <Button size="small" danger icon={<StopOutlined />} />
                  </Popconfirm>
                ),
              },
            ]}
            locale={{ emptyText: <Empty description="暂无运行中任务" image={Empty.PRESENTED_IMAGE_SIMPLE} /> }}
          />
        </div>
      ),
    },
    {
      key: 'messages',
      label: (
        <span>
          <MessageOutlined style={{ marginRight: 6 }} />
          消息
        </span>
      ),
      children: (
        <Tabs
          defaultActiveKey="bind"
          size="small"
          items={[
            {
              key: 'bind',
              label: '绑定',
              children: (
                <div className="settings-messages-tab" style={{ maxWidth: 700 }}>
                  <Card
                    title="绑定消息接收智能体"
                    size="small"
                    style={{ marginBottom: 24 }}
                    extra={
                      <Button
                        type="primary"
                        icon={<QrcodeOutlined />}
                        onClick={handleStartFeishuBind}
                        loading={binding}
                        size="small"
                      >
                        绑定飞书
                      </Button>
                    }
                  >
                    <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
                      绑定飞书智能体 Bot 后，可以接收任务执行结果和通知消息。支持绑定多个 Bot。
                    </Paragraph>

                    <Spin spinning={botsLoading}>
                      {agentBots.length === 0 ? (
                        <Empty description="暂无绑定的智能体" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                      ) : (
                        <List
                          dataSource={agentBots}
                          renderItem={(bot) => {
                            let botConfig: Record<string, boolean> = { dm_enabled: true, group_enabled: true, group_require_mention: true, echo_reply: true };
                            try { botConfig = JSON.parse(bot.config || '{}'); } catch {}
                            const isFeishu = bot.bot_type === 'feishu';
                            const handleConfigChange = async (key: string, val: boolean) => {
                              const newConfig = { ...botConfig, [key]: val };
                              try {
                                await db.updateAgentBotConfig(bot.id, JSON.stringify(newConfig));
                                setAgentBots(prev => prev.map(b => b.id === bot.id ? { ...b, config: JSON.stringify(newConfig) } : b));
                              } catch (e: any) {
                                message.error('保存配置失败: ' + (e.message || '未知错误'));
                              }
                            };

                            const botPushStatus = feishuPushStatus.find(p => p.bot_id === bot.id);
                            const hasPushTarget = !!botPushStatus;
                            const handlePushLevelChange = async (level: db.FeishuPushLevel) => {
                              try {
                                await db.updateFeishuPush({ botId: bot.id, pushLevel: level });
                                loadFeishuPush();
                              } catch (e: any) {
                                message.error('设置推送失败: ' + (e.message || '未知错误'));
                              }
                            };
                            const handlePushTargetUpdate = async (field: 'p2p_receive_id' | 'receive_id_type' | 'group_chat_id', value: string) => {
                              try {
                                const updateField = field === 'p2p_receive_id' ? 'p2pReceiveId'
                                  : field === 'group_chat_id' ? 'groupChatId' : 'receiveIdType';
                                await db.updateFeishuPush({ botId: bot.id, [updateField]: value });
                                loadFeishuPush();
                              } catch (e: any) {
                                message.error('更新推送目标失败: ' + (e.message || '未知错误'));
                              }
                            };
                            const handleResponseEnabledChange = async (botId: number, targetType: 'p2p' | 'group', enabled: boolean) => {
                              try {
                                if (targetType === 'p2p') {
                                  await db.updateFeishuPush({ botId, p2pResponseEnabled: enabled });
                                } else {
                                  await db.updateFeishuPush({ botId, groupResponseEnabled: enabled });
                                }
                                loadFeishuPush();
                              } catch (e: any) {
                                message.error('更新响应开关失败: ' + (e.message || '未知错误'));
                              }
                            };
                            const copyToClipboard = (text: string, label: string) => {
                              navigator.clipboard.writeText(text).then(() => {
                                message.success(`${label} 已复制`);
                              }).catch(() => {
                                message.error('复制失败');
                              });
                            };

                            return (
                              <div
                                key={bot.id}
                                style={{
                                  padding: '12px',
                                  background: 'var(--color-bg)',
                                  borderRadius: 8,
                                  marginBottom: 8,
                                  border: '1px solid var(--color-border-light)',
                                }}
                              >
                                <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
                                  <div
                                    style={{
                                      width: 36,
                                      height: 36,
                                      borderRadius: 8,
                                      background: isFeishu ? '#1976D2' : '#888',
                                      display: 'flex',
                                      alignItems: 'center',
                                      justifyContent: 'center',
                                      color: '#fff',
                                      fontWeight: 700,
                                      fontSize: 14,
                                      flexShrink: 0,
                                    }}
                                  >
                                    {isFeishu ? '飞' : '其他'}
                                  </div>
                                  <div style={{ flex: 1, minWidth: 0 }}>
                                    <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                                      <span style={{ fontWeight: 600, fontSize: 14, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{bot.bot_name}</span>
                                      <AntTag color={bot.enabled ? 'green' : 'default'} style={{ marginRight: 0 }}>
                                        {bot.enabled ? '已启用' : '已禁用'}
                                      </AntTag>
                                    </div>
                                    <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', wordBreak: 'break-all', lineHeight: 1.6 }}>
                                      App ID: {bot.app_id}
                                    </div>
                                    {bot.domain && (
                                      <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
                                        平台: {bot.domain === 'lark' ? 'Lark 国际版' : '飞书'}
                                      </div>
                                    )}
                                    <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 2 }}>
                                      绑定时间: {new Date(bot.created_at).toLocaleString()}
                                    </div>
                                  </div>
                                  <Popconfirm
                                    title="删除确认"
                                    description={`确定要删除 "${bot.bot_name}" 吗？`}
                                    onConfirm={() => handleDeleteBot(bot.id)}
                                    okText="删除"
                                    cancelText="取消"
                                    okButtonProps={{ danger: true }}
                                  >
                                    <Button type="text" danger icon={<DeleteOutlined />} size="small" style={{ flexShrink: 0 }} />
                                  </Popconfirm>
                                </div>
                                {isFeishu && (
                                  <div style={{ marginTop: 8, paddingTop: 8, borderTop: '1px solid var(--color-border-light)' }}>
                                    <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 6 }}>消息配置</div>
                                    <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px 16px' }}>
                                      <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                        <Switch size="small" checked={botConfig.dm_enabled !== false} onChange={v => handleConfigChange('dm_enabled', v)} />
                                        接收单聊消息
                                      </span>
                                      <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                        <Switch size="small" checked={botConfig.group_enabled !== false} onChange={v => handleConfigChange('group_enabled', v)} />
                                        接收群聊消息
                                      </span>
                                      <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                        <Switch size="small" checked={botConfig.group_require_mention !== false} onChange={v => handleConfigChange('group_require_mention', v)} />
                                        群聊仅处理@
                                      </span>
                                      <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                        <Switch size="small" checked={botConfig.echo_reply !== false} onChange={v => handleConfigChange('echo_reply', v)} />
                                        Echo 回复
                                      </span>
                                      {hasPushTarget && (<>
                                        <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                          <Switch
                                            size="small"
                                            checked={botPushStatus.p2p_response_enabled}
                                            onChange={(v) => handleResponseEnabledChange(botPushStatus.bot_id, 'p2p', v)}
                                          />
                                          单聊响应
                                          <InputNumber
                                            size="small"
                                            min={1}
                                            max={300}
                                            value={botPushStatus.p2p_debounce_secs}
                                            onChange={(v) => { if (v !== null) db.updateFeishuPush({ botId: botPushStatus.bot_id, p2pDebounceSecs: v }); }}
                                            style={{ width: 50, fontSize: 11 }}
                                          />
                                          <span style={{ fontSize: 10 }}>秒合并</span>
                                        </span>
                                        <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                          <Switch
                                            size="small"
                                            checked={botPushStatus.group_response_enabled}
                                            onChange={(v) => handleResponseEnabledChange(botPushStatus.bot_id, 'group', v)}
                                          />
                                          群聊响应
                                          <InputNumber
                                            size="small"
                                            min={1}
                                            max={300}
                                            value={botPushStatus.group_debounce_secs}
                                            onChange={(v) => { if (v !== null) db.updateFeishuPush({ botId: botPushStatus.bot_id, groupDebounceSecs: v }); }}
                                            style={{ width: 50, fontSize: 11 }}
                                          />
                                          <span style={{ fontSize: 10 }}>秒合并</span>
                                        </span>
                                      </>)}
                                    </div>
                                    {hasPushTarget && (
                                      <div style={{ marginTop: 10 }}>
                                        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 6 }}>
                                          推送目标
                                          <Select
                                            size="small"
                                            value={botPushStatus.push_level}
                                            onChange={handlePushLevelChange}
                                            style={{ width: 90, marginLeft: 8 }}
                                            options={[
                                              { value: 'disabled', label: '关闭' },
                                              { value: 'result_only', label: '仅结论' },
                                              { value: 'all', label: '全部' },
                                            ]}
                                          />
                                        </div>
                                        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                                          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                            <span style={{ fontSize: 11, width: 80, color: 'var(--color-text-tertiary)' }}>单聊ID:</span>
                                            <Input
                                              size="small"
                                              value={botPushStatus.p2p_receive_id}
                                              onChange={(e) => handlePushTargetUpdate('p2p_receive_id', e.target.value)}
                                              style={{ flex: 1, fontSize: 11 }}
                                            />
                                            <Button size="small" icon={<CopyOutlined />} onClick={() => copyToClipboard(botPushStatus.p2p_receive_id, 'p2p_receive_id')} />
                                          </div>
                                          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                            <span style={{ fontSize: 11, width: 80, color: 'var(--color-text-tertiary)' }}>群ID:</span>
                                            <Input
                                              size="small"
                                              value={botPushStatus.group_chat_id || ''}
                                              onChange={(e) => handlePushTargetUpdate('group_chat_id', e.target.value)}
                                              style={{ flex: 1, fontSize: 11 }}
                                            />
                                            <Button size="small" icon={<CopyOutlined />} onClick={() => copyToClipboard(botPushStatus.group_chat_id || '', 'group_chat_id')} />
                                          </div>
                                          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                            <span style={{ fontSize: 11, width: 80, color: 'var(--color-text-tertiary)' }}>发送类型:</span>
                                            <Select
                                              size="small"
                                              value={botPushStatus.receive_id_type}
                                              onChange={(v) => handlePushTargetUpdate('receive_id_type', v)}
                                              style={{ width: 100 }}
                                              options={[
                                                { value: 'open_id', label: '私聊' },
                                                { value: 'chat_id', label: '群聊' },
                                              ]}
                                            />
                                            <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginLeft: 84 }}>
                                              提示：向机器人发送 /sethome 可快速设置当前对话的 ID
                                            </span>
                                          </div>
                                        </div>
                                      </div>
                                    )}
                                    {hasPushTarget && (
                                      <div style={{ marginTop: 10 }}>
                                        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>
                                          群聊响应白名单
                                          <Tooltip title="白名单为空时不限制，仅白名单内的用户消息会触发响应">
                                            <InfoCircleOutlined style={{ marginLeft: 4, fontSize: 10 }} />
                                          </Tooltip>
                                        </div>
                                        <div style={{ display: 'flex', gap: 4, marginBottom: 4 }}>
                                          <AutoComplete
                                            size="small"
                                            placeholder="搜索或粘贴 Open ID"
                                            value={whitelistBotId === botPushStatus.bot_id ? whitelistOpenId : undefined}
                                            onChange={(v) => { setWhitelistBotId(botPushStatus.bot_id); setWhitelistOpenId(v); }}
                                            onFocus={() => { if (whitelistBotId !== botPushStatus.bot_id) { loadGroupWhitelist(botPushStatus.bot_id); loadHistorySenders(); } }}
                                            filterOption={(input, option) => {
                                              if (!option?.value) return false;
                                              const val = (option.value as string).toLowerCase();
                                              const label = (option.label as string)?.toLowerCase() || '';
                                              const q = input.toLowerCase();
                                              return val.includes(q) || label.includes(q);
                                            }}
                                            style={{ flex: 1, fontSize: 11 }}
                                            options={historySenders
                                              .filter(s => s.sender_open_id)
                                              .map((s) => {
                                                const typeTag = s.sender_type === 'app' ? '[Bot] ' : '';
                                                const label = s.sender_nickname || s.sender_open_id;
                                                return {
                                                  value: s.sender_open_id,
                                                  label: `${typeTag}${label} (${s.count}条)`,
                                                };
                                              })
                                            }
                                          />
                                          <Input
                                            size="small"
                                            placeholder="备注名"
                                            value={whitelistBotId === botPushStatus.bot_id ? whitelistName : ''}
                                            onChange={(e) => { setWhitelistBotId(botPushStatus.bot_id); setWhitelistName(e.target.value); }}
                                            style={{ width: 80, fontSize: 11 }}
                                          />
                                          <Button size="small" onClick={handleAddWhitelist}>添加</Button>
                                        </div>
                                        {(whitelistBotId === botPushStatus.bot_id ? groupWhitelist : []).map((w) => (
                                          <div key={w.id} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 11, marginBottom: 2 }}>
                                            <span style={{ color: 'var(--color-text)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                              {w.sender_name || w.sender_open_id}
                                            </span>
                                            <span style={{ color: 'var(--color-text-tertiary)', fontSize: 10 }}>{w.sender_open_id.slice(0, 12)}...</span>
                                            <Button size="small" danger type="link" style={{ fontSize: 10, padding: 0 }} onClick={() => handleDeleteWhitelist(w.id)}>删除</Button>
                                          </div>
                                        ))}
                                        {(whitelistBotId === botPushStatus.bot_id ? groupWhitelist : []).length === 0 && (
                                          <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>暂无白名单，所有用户均可触发响应</div>
                                        )}
                                      </div>
                                    )}
                                  </div>
                                )}
                              </div>
                            );
                          }}
                        />
                      )}
                    </Spin>
                  </Card>

                  <Modal
                    open={!!historyViewMsg}
                    onCancel={() => setHistoryViewMsg(null)}
                    footer={null}
                    width={560}
                    title="消息详情"
                  >
                    <div style={{ fontSize: 13, lineHeight: 1.8, whiteSpace: 'pre-wrap', wordBreak: 'break-all', maxHeight: 400, overflowY: 'auto' }}>
                      {historyViewMsg}
                    </div>
                  </Modal>

                  <Card
                    title="斜杠命令规则"
                    size="small"
                    style={{ marginBottom: 24 }}
                    extra={
                      <Button type="primary" size="small" onClick={handleSaveConfig} loading={configSaving}>
                        保存规则
                      </Button>
                    }
                  >
                    <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
                      配置全局斜杠命令，将飞书消息中的命令路由到指定 Todo。命中后会把命令后的正文作为参数传入 Todo Prompt，支持使用 {'{{'}content{'}}'}、{'{{'}message{'}}'}、{'{{'}raw_message{'}}'}、{'{{'}slash_command{'}}'}。
                    </Paragraph>
                    <Form form={configForm} layout="vertical">
                      <Form.List name="slash_command_rules">
                        {(fields, { add, remove }) => (
                          <>
                            {fields.length === 0 && (
                              <Empty
                                image={Empty.PRESENTED_IMAGE_SIMPLE}
                                description="暂无规则，点击下方按钮新增"
                                style={{ margin: '12px 0 20px' }}
                              />
                            )}
                            {fields.map((field, index) => (
                              <Card
                                key={field.key}
                                size="small"
                                style={{ marginBottom: 12, background: 'var(--color-bg)' }}
                                title={`规则 ${index + 1}`}
                                extra={
                                  <Button
                                    type="text"
                                    danger
                                    size="small"
                                    icon={<MinusCircleOutlined />}
                                    onClick={() => remove(field.name)}
                                  />
                                }
                              >
                                <Form.Item
                                  name={[field.name, 'slash_command']}
                                  label="斜杠命令"
                                  rules={[
                                    { required: true, message: '请输入斜杠命令' },
                                    {
                                      validator: (_, value) => {
                                        const command = String(value || '').trim();
                                        if (!command) return Promise.resolve();
                                        if (!/^\/\S+$/.test(command)) {
                                          return Promise.reject(new Error('命令必须以 / 开头，且不能包含空格'));
                                        }
                                        return Promise.resolve();
                                      },
                                    },
                                  ]}
                                >
                                  <Input placeholder="/todo" />
                                </Form.Item>
                                <Form.Item
                                  name={[field.name, 'todo_id']}
                                  label="目标 Todo"
                                  rules={[{ required: true, message: '请选择目标 Todo' }]}
                                >
                                  <Select
                                    showSearch
                                    placeholder="搜索并选择 Todo"
                                    optionFilterProp="label"
                                    options={todos.map((todo) => ({
                                      value: todo.id,
                                      label: `#${todo.id} ${todo.title}`,
                                    }))}
                                  />
                                </Form.Item>
                                <Form.Item
                                  name={[field.name, 'enabled']}
                                  label="启用"
                                  valuePropName="checked"
                                  initialValue={true}
                                >
                                  <Switch size="small" />
                                </Form.Item>
                              </Card>
                            ))}
                            <Button
                              block
                              icon={<PlusOutlined />}
                              onClick={() => add({ slash_command: '', todo_id: undefined, enabled: true })}
                            >
                              新增规则
                            </Button>
                          </>
                        )}
                      </Form.List>
                    </Form>
                  </Card>

                  <Card
                    title="默认响应"
                    size="small"
                    style={{ marginBottom: 24 }}
                    extra={
                      <Button type="primary" size="small" onClick={handleSaveConfig} loading={configSaving}>
                        保存
                      </Button>
                    }
                  >
                    <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
                      当收到的消息没有匹配到任何斜杠命令时，执行默认响应的 Todo。支持使用 {'{{'}content{'}}'}、{'{{'}message{'}}'}、{'{{'}raw_message{'}}'}、{'{{'}slash_command{'}}'} 参数。
                    </Paragraph>
                    <Form form={configForm} layout="vertical" style={{ maxWidth: 400 }}>
                      <Form.Item
                        name="default_response_todo_id"
                        label="默认响应 Todo"
                      >
                        <Select
                          showSearch
                          allowClear
                          placeholder="选择默认响应的 Todo"
                          optionFilterProp="label"
                          options={todos.map((todo) => ({
                            value: todo.id,
                            label: `#${todo.id} ${todo.title}`,
                          }))}
                        />
                      </Form.Item>
                    </Form>
                  </Card>

                  <Card
                    title="历史消息处理"
                    size="small"
                    style={{ marginBottom: 24 }}
                    extra={
                      <Button type="primary" size="small" onClick={handleSaveConfig} loading={configSaving}>
                        保存
                      </Button>
                    }
                  >
                    <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
                      拉取历史消息时，超过设定时间的消息将保存但跳过处理，避免离线后重新处理大量旧消息。
                    </Paragraph>
                    <Form form={configForm} layout="vertical" style={{ maxWidth: 400 }}>
                      <Form.Item
                        name="history_message_max_age_secs"
                        label="最大处理年龄（秒）"
                        tooltip="仅处理此时间内的历史消息，默认 600 秒（10 分钟）"
                      >
                        <InputNumber
                          min={0}
                          max={86400}
                          step={60}
                          placeholder="600"
                          addonAfter="秒"
                          style={{ width: '100%' }}
                        />
                      </Form.Item>
                    </Form>
                  </Card>

                  <Modal
                    title={
                      <Space>
                        <QrcodeOutlined />
                        绑定飞书智能体
                      </Space>
                    }
                    open={bindModalOpen}
                    onCancel={() => {
                      setBindModalOpen(false);
                      setQrCodeUrl('');
                      setPollError('');
                      setBindSuccess(false);
                    }}
                    footer={null}
                    width={400}
                    centered
                    className="settings-bind-modal"
                  >
                    <div style={{ textAlign: 'center', padding: '16px 0' }}>
                      {pollError && (
                        <div style={{ marginBottom: 16, color: '#ff4d4f', fontSize: 13 }}>
                          {pollError}
                        </div>
                      )}

                      {bindSuccess ? (
                        <div style={{ color: '#52c41a', fontSize: 48, marginBottom: 16 }}>
                          ✓
                        </div>
                      ) : (
                        <>
                          {qrCodeUrl ? (
                            <div style={{ marginBottom: 16 }}>
                              <img src={qrCodeUrl} alt="QR Code" style={{ width: '100%', maxWidth: 200, height: 'auto' }} />
                              <div style={{ marginTop: 12, color: 'var(--color-text-secondary)', fontSize: 13 }}>
                                请使用飞书 App 扫描二维码绑定
                              </div>
                              <div style={{ marginTop: 6, fontSize: 12, color: 'var(--color-text-tertiary)' }}>
                                二维码有效期 10 分钟，请尽快完成
                              </div>
                            </div>
                          ) : (
                            <Spin size="large" />
                          )}
                        </>
                      )}

                      {binding && !qrCodeUrl && (
                        <div style={{ marginTop: 16, color: 'var(--color-text-secondary)', fontSize: 13 }}>
                          正在生成二维码...
                        </div>
                      )}
                    </div>
                  </Modal>
                </div>
              ),
            },
            {
              key: 'record',
              label: '记录',
              children: (
                <div className="settings-history-tab">
                  <div
                    style={{
                      marginBottom: 16,
                      display: 'flex',
                      flexWrap: 'wrap',
                      gap: 8,
                      justifyContent: 'space-between',
                      alignItems: 'center',
                    }}
                  >
                    <Space>
                      <HistoryOutlined />
                      <span style={{ fontWeight: 600 }}>飞书群聊消息</span>
                      <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                        <Tooltip title={
                          <div style={{ fontSize: 12, lineHeight: 1.6 }}>
                            <b>实时消息：</b>通过 WebSocket 实时接收的消息，接收后立即处理并触发相关事件<br/>
                            <b>历史消息：</b>通过轮询 API 拉取的群聊历史记录，不会触发实时处理事件
                          </div>
                        }>
                          <span style={{ cursor: 'help' }}><QuestionCircleOutlined /></span>
                        </Tooltip>
                      </Typography.Text>
                    </Space>
                    <Space wrap>
                      <Select
                        placeholder="筛选群聊"
                        allowClear
                        style={{ width: 200 }}
                        value={historySelectedChatId}
                        onChange={(v) => {
                          setHistorySelectedChatId(v);
                          setHistoryPage(1);
                        }}
                        onClear={() => {
                          setHistorySelectedChatId(undefined);
                          setHistoryPage(1);
                        }}
                      >
                        {historyChats.map((chat) => (
                          <Select.Option key={chat.chat_id} value={chat.chat_id}>
                            {chat.chat_name || chat.chat_id}
                          </Select.Option>
                        ))}
                      </Select>
                      <Select
                        placeholder="筛选发送者"
                        allowClear
                        style={{ width: 150 }}
                        value={historySelectedSenderId}
                        onChange={(v) => {
                          setHistorySelectedSenderId(v);
                          setHistoryPage(1);
                        }}
                        onClear={() => {
                          setHistorySelectedSenderId(undefined);
                          setHistoryPage(1);
                        }}
                      >
                        {historySenders.map((item) => (
                          <Select.Option key={item.sender_open_id} value={item.sender_open_id}>
                            {item.sender_nickname || item.sender_open_id.slice(0, 12)} ({item.count}条)
                          </Select.Option>
                        ))}
                      </Select>
                      <Select
                        placeholder="消息来源"
                        style={{ width: 130 }}
                        value={historyIsHistory}
                        onChange={(v) => {
                          setHistoryIsHistory(v);
                          setHistoryPage(1);
                        }}
                        allowClear
                      >
                        <Select.Option value={true}>仅历史消息</Select.Option>
                        <Select.Option value={false}>仅实时消息</Select.Option>
                      </Select>
                      <Button icon={<ReloadOutlined />} onClick={loadHistoryMessages} size="middle">
                        刷新
                      </Button>
                      <Button type="primary" icon={<PlusOutlined />} onClick={() => setHistoryAddModalOpen(true)} size="middle">
                        添加
                      </Button>
                    </Space>
                  </div>

                  <Table
                    dataSource={historyMessages}
                    rowKey="id"
                    loading={historyLoading}
                    scroll={{ x: 'max-content' }}
                    pagination={{
                      current: historyPage,
                      pageSize: historyPageSize,
                      total: historyTotal,
                      showSizeChanger: true,
                      showQuickJumper: true,
                      showTotal: (t) => `共 ${t} 条`,
                      onChange: (p, ps) => {
                        setHistoryPage(p);
                        setHistoryPageSize(ps);
                      },
                    }}
                    size="middle"
                    columns={[
                      {
                        title: '时间',
                        dataIndex: 'created_at',
                        key: 'created_at',
                        width: 150,
                        render: (text: string) => {
                          if (!text) return '-';
                          const d = new Date(text);
                          return isNaN(d.getTime()) ? text : d.toLocaleString('zh-CN');
                        },
                      },
                      {
                        title: '来源',
                        key: 'source',
                        width: 90,
                        render: (_, record) => (
                          <AntTag color={record.is_history ? 'orange' : 'cyan'}>
                            {record.is_history ? '历史' : '实时'}
                          </AntTag>
                        ),
                      },
                      {
                        title: '发送者',
                        key: 'sender',
                        width: 160,
                        render: (_, record) => {
                          const isBot = record.sender_type === 'app';
                          return (
                            <Space size={2}>
                              <AntTag color={isBot ? 'blue' : 'green'}>
                                {isBot ? '智能体' : '用户'}
                              </AntTag>
                              <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                                {record.sender_nickname || record.sender_open_id?.slice(0, 8) || '-'}
                              </Typography.Text>
                              {record.sender_open_id && (
                                <Button
                                  size="small"
                                  type="link"
                                  icon={<CopyOutlined />}
                                  style={{ fontSize: 10, padding: 0 }}
                                  onClick={() => {
                                    navigator.clipboard.writeText(record.sender_open_id);
                                    message.success('已复制 Open ID');
                                  }}
                                />
                              )}
                            </Space>
                          );
                        },
                      },
                      {
                        title: '内容',
                        dataIndex: 'content',
                        key: 'content',
                        width: 200,
                        render: (content: string, record) => {
                          let text: string;
                          if (record.msg_type === 'text') {
                            try {
                              const parsed = JSON.parse(content);
                              text = parsed.text || content;
                            } catch {
                              text = content;
                            }
                          } else {
                            return <AntTag>{record.msg_type}</AntTag>;
                          }
                          const MAX = 40;
                          const truncated = text.length > MAX ? text.slice(0, MAX) + '...' : text;
                          return (
                            <span
                              style={{ cursor: 'pointer', fontSize: 12 }}
                              onClick={() => setHistoryViewMsg(text)}
                            >
                              {truncated}
                            </span>
                          );
                        },
                      },
                      {
                        title: '处理状态',
                        key: 'processed',
                        width: 90,
                        render: (_, record) => (
                          record.processed ? (
                            <AntTag color="green">已处理</AntTag>
                          ) : (
                            <AntTag color="default">未处理</AntTag>
                          )
                        ),
                      },
                      {
                        title: '触发Todo',
                        key: 'processed_todo_id',
                        width: 80,
                        render: (_, record) => (
                          record.processed_todo_id ? (
                            <Typography.Link
                              style={{ fontSize: 12 }}
                              onClick={() => {
                                dispatch({ type: 'SELECT_TODO', payload: record.processed_todo_id });
                                onBack?.();
                              }}
                            >
                              #{record.processed_todo_id}
                            </Typography.Link>
                          ) : (
                            <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
                          )
                        ),
                      },
                      {
                        title: '执行记录',
                        key: 'execution_record_id',
                        width: 80,
                        render: (_, record) => (
                          record.execution_record_id ? (
                            <Typography.Link
                              style={{ fontSize: 12 }}
                              onClick={() => {
                                db.getExecutionRecord(record.execution_record_id!)
                                  .then(r => setExecDetailRecord(r))
                                  .catch((err) => {
                                    message.error('加载执行记录失败: ' + (err instanceof Error ? err.message : '未知错误'));
                                  });
                              }}
                            >
                              #{record.execution_record_id}
                            </Typography.Link>
                          ) : (
                            <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
                          )
                        ),
                      },
                    ]}
                  />

                  <Modal
                    title="添加监听群聊"
                    open={historyAddModalOpen}
                    onOk={handleAddHistoryChat}
                    onCancel={() => {
                      setHistoryAddModalOpen(false);
                      historyForm.resetFields();
                    }}
                    width={520}
                  >
                    <Form form={historyForm} layout="vertical">
                      <Form.Item
                        name="bot_id"
                        label="机器人"
                        rules={[{ required: true, message: '请选择机器人' }]}
                      >
                        <Select placeholder="请选择机器人">
                          {agentBots.filter(b => b.bot_type === 'feishu').map((bot) => (
                            <Select.Option key={bot.id} value={bot.id}>
                              {bot.bot_name}
                            </Select.Option>
                          ))}
                        </Select>
                      </Form.Item>
                      <Form.Item
                        name="chat_id"
                        label="群聊 ID"
                        rules={[{ required: true, message: '请输入群聊 ID' }]}
                      >
                        <Input placeholder="请输入飞书群聊 ID" />
                      </Form.Item>
                      <Form.Item name="chat_name" label="群聊名称（可选）">
                        <Input placeholder="请输入群聊名称，方便识别" />
                      </Form.Item>
                    </Form>
                  </Modal>
                </div>
              ),
            },
          ]}
        />
      ),
    },
    {
      key: 'sessions',
      label: (
        <span>
          <LaptopOutlined style={{ marginRight: 6 }} />
          Session 管理
        </span>
      ),
      children: <SessionManager />,
    },
    {
      key: 'templates',
      label: (
        <span>
          <FileTextOutlined style={{ marginRight: 6 }} />
          模板管理
        </span>
      ),
      children: (
        <div style={{ maxWidth: 700 }}>
          <Spin spinning={templatesLoading}>
            <Tabs
              defaultActiveKey="user"
              items={[
                {
                  key: 'user',
                  label: '我的模板',
                  children: (
                    <div>
                      <div style={{ marginBottom: 16 }}>
                        <Button type="primary" icon={<PlusOutlined />} onClick={() => openTemplateForm()}>
                          新建模板
                        </Button>
                      </div>
                      {templates.filter(t => !t.is_system && !t.source_url).length === 0 ? (
                        <Empty description="暂无用户模板" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                      ) : (
                        Array.from(new Set(templates.filter(t => !t.is_system && !t.source_url).map(t => t.category))).sort().map(category => (
                          <Card key={category} title={category || '未分类'} size="small" style={{ marginBottom: 12 }}>
                            <List
                              dataSource={templates.filter(t => !t.is_system && !t.source_url && t.category === category)}
                              renderItem={(template) => (
                                <List.Item
                                  style={{ padding: '8px 0' }}
                                  actions={[
                                    <Button key="edit" type="text" icon={<EditOutlined />} size="small" onClick={() => openTemplateForm(template)} />,
                                    <Popconfirm key="delete" title="删除模板" description={`确定要删除模板 "${template.title}" 吗？`} onConfirm={() => handleDeleteTemplate(template.id)}>
                                      <Button type="text" danger icon={<DeleteOutlined />} size="small" />
                                    </Popconfirm>,
                                  ]}
                                >
                                  <List.Item.Meta
                                    title={template.title}
                                    description={template.prompt || '(无内容)'}
                                  />
                                </List.Item>
                              )}
                            />
                          </Card>
                        ))
                      )}
                    </div>
                  ),
                },
                {
                  key: 'system',
                  label: '系统模板',
                  children: (
                    <div>
                      {templates.filter(t => t.is_system).length === 0 ? (
                        <Empty description="暂无系统模板" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                      ) : (
                        Array.from(new Set(templates.filter(t => t.is_system).map(t => t.category))).sort().map(category => (
                          <Card key={category} title={category || '未分类'} size="small" style={{ marginBottom: 12 }}>
                            <List
                              dataSource={templates.filter(t => t.is_system && t.category === category)}
                              renderItem={(template) => (
                                <List.Item
                                  style={{ padding: '8px 0' }}
                                  actions={[
                                    <Button key="copy" type="text" icon={<CopyOutlined />} size="small" onClick={() => handleCopyTemplate(template.id)}>
                                      复制
                                    </Button>,
                                  ]}
                                >
                                  <List.Item.Meta
                                    title={template.title}
                                    description={template.prompt || '(无内容)'}
                                  />
                                </List.Item>
                              )}
                            />
                          </Card>
                        ))
                      )}
                    </div>
                  ),
                },
                {
                  key: 'custom',
                  label: '自定义',
                  children: (
                    <div>
                      <Spin spinning={customTemplateLoading}>
                        {customTemplateStatus?.subscribed ? (
                          <div>
                            <Card size="small" style={{ marginBottom: 12 }}>
                              <Space direction="vertical" style={{ width: '100%' }}>
                                <div>
                                  <Typography.Text type="secondary">订阅地址：</Typography.Text>
                                  <Typography.Text copyable>{customTemplateStatus.source_url}</Typography.Text>
                                </div>
                                {customTemplateStatus.last_sync_at && (
                                  <div>
                                    <Typography.Text type="secondary">最后同步：</Typography.Text>
                                    <Typography.Text>{new Date(customTemplateStatus.last_sync_at).toLocaleString()}</Typography.Text>
                                  </div>
                                )}
                                <Space>
                                  <Button icon={<ReloadOutlined />} onClick={handleSyncCustomTemplate}>
                                    立即同步
                                  </Button>
                                  <Popconfirm
                                    title="取消订阅"
                                    description="确定要取消订阅吗？订阅的模板将被删除。"
                                    onConfirm={handleUnsubscribeCustomTemplate}
                                  >
                                    <Button danger>取消订阅</Button>
                                  </Popconfirm>
                                </Space>
                              </Space>
                            </Card>

                            <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 12, marginTop: 4 }}>
                              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                                <span style={{ fontWeight: 600 }}><ClockCircleOutlined style={{ marginRight: 6 }} />自动同步</span>
                                <Switch checked={customTemplateAutoSyncEnabled} onChange={setCustomTemplateAutoSyncEnabled} />
                              </div>
                              {customTemplateAutoSyncEnabled && (
                                <CronPresetSelect
                                  value={customTemplateAutoSyncCron}
                                  onChange={(val) => setCustomTemplateAutoSyncCron(val)}
                                />
                              )}
                              {customTemplateAutoSyncEnabled && (
                                <Cron
                                  value={cronTo5(customTemplateAutoSyncCron)}
                                  setValue={(val: string) => setCustomTemplateAutoSyncCron(cronTo6(val))}
                                  locale={CRON_ZH_LOCALE}
                                  defaultPeriod="day"
                                  humanizeLabels
                                  allowClear={false}
                                />
                              )}
                              {customTemplateAutoSyncEnabled && (
                                <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
                                  <Button size="small" type="primary" onClick={handleUpdateCustomTemplateAutoSync}>
                                    保存
                                  </Button>
                                </div>
                              )}
                            </div>

                            <div style={{ marginBottom: 8 }}>
                              <Typography.Text strong>模板列表</Typography.Text>
                            </div>
                            {customTemplateStatus.templates.length === 0 ? (
                              <Empty description="暂无自定义模板" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                            ) : (
                              Array.from(new Set(customTemplateStatus.templates.map(t => t.category))).sort().map(category => (
                                <Card key={category} title={category || '未分类'} size="small" style={{ marginBottom: 12 }}>
                                  <List
                                    dataSource={customTemplateStatus.templates.filter(t => t.category === category)}
                                    renderItem={(template) => (
                                      <List.Item
                                        style={{ padding: '8px 0' }}
                                        actions={[
                                          <Button key="copy" type="text" icon={<CopyOutlined />} size="small" onClick={() => handleCopyTemplate(template.id)}>
                                            复制
                                          </Button>,
                                        ]}
                                      >
                                        <List.Item.Meta
                                          title={template.title}
                                          description={template.prompt || '(无内容)'}
                                        />
                                      </List.Item>
                                    )}
                                  />
                                </Card>
                              ))
                            )}
                          </div>
                        ) : (
                          <Card size="small">
                            <Space direction="vertical" style={{ width: '100%' }}>
                              <Space>
                                <Typography.Text>订阅一个在线模板地址</Typography.Text>
                                <Tooltip title={<span>填写在线 YAML 地址，格式参考 GitHub <a href="https://raw.githubusercontent.com/weibaohui/nothing-todo/refs/heads/main/templates.example.yaml" target="_blank">示例</a> 或 GitCode <a href="https://raw.gitcode.com/weibaohui/nothing-todo/raw/main/templates.example.yaml" target="_blank">示例</a></span>}>
                                  <span style={{ cursor: 'help' }}><QuestionCircleOutlined /></span>
                                </Tooltip>
                              </Space>
                              <Input
                                placeholder="输入模板地址"
                                value={customTemplateUrl}
                                onChange={(e) => setCustomTemplateUrl(e.target.value)}
                                onPressEnter={handleSubscribeCustomTemplate}
                              />
                              <Button
                                type="primary"
                                loading={customTemplateSubscribing}
                                onClick={handleSubscribeCustomTemplate}
                              >
                                订阅
                              </Button>
                            </Space>
                          </Card>
                        )}
                      </Spin>
                    </div>
                  ),
                },
              ]}
            />
          </Spin>
          <Modal
            title={templateEditing ? '编辑模板' : '新建模板'}
            open={templateFormOpen}
            onOk={handleSaveTemplate}
            onCancel={closeTemplateForm}
            confirmLoading={templateFormSaving}
            width={500}
          >
            <Space direction="vertical" style={{ width: '100%' }}>
              <div>
                <div style={{ marginBottom: 4, fontWeight: 500 }}>标题</div>
                <Input
                  value={templateFormTitle}
                  onChange={e => setTemplateFormTitle(e.target.value)}
                  placeholder="输入模板标题"
                />
              </div>
              <div>
                <div style={{ marginBottom: 4, fontWeight: 500 }}>分类</div>
                <AutoComplete
                  placeholder="输入或选择分类"
                  value={templateFormCategory}
                  onChange={(value) => setTemplateFormCategory(value)}
                  options={Array.from(new Set(templates.map(t => t.category))).filter(c => c).map(c => ({ label: c, value: c }))}
                  style={{ width: '100%' }}
                  filterOption={(input, option) =>
                    (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
                  }
                />
              </div>
              <div>
                <div style={{ marginBottom: 4, fontWeight: 500 }}>Prompt 内容</div>
                <Input.TextArea
                  value={templateFormPrompt}
                  onChange={e => setTemplateFormPrompt(e.target.value)}
                  placeholder="输入模板的 prompt 内容（可选）"
                  rows={6}
                />
              </div>
            </Space>
          </Modal>
        </div>
      ),
    },
    {
      key: 'webhooks',
      label: (
        <span>
          <ApiOutlined style={{ marginRight: 6 }} />
          Webhook
        </span>
      ),
      children: <WebhooksPanel todos={todos} />,
    },
    {
      key: 'about',
      label: (
        <span>
          <InfoCircleOutlined style={{ marginRight: 6 }} />
          关于
        </span>
      ),
      children: (
        <Spin spinning={versionLoading}>
          <Card title="NTD 版本信息" style={{ maxWidth: 600 }}>
            {versionInfo ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
                <div>
                  <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>版本号</div>
                  <div style={{ fontSize: 24, fontWeight: 700, fontFamily: 'monospace' }}>{versionInfo.version}</div>
                </div>
                <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 16 }}>
                  <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 8 }}>详细信息</div>
                  <Space direction="vertical" size={8}>
                    <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                      <span style={{ fontWeight: 500, minWidth: 80 }}>Git SHA:</span>
                      <code style={{ background: 'var(--color-bg-elevated)', padding: '2px 8px', borderRadius: 4, fontFamily: 'monospace' }}>{versionInfo.git_sha}</code>
                    </div>
                    <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                      <span style={{ fontWeight: 500, minWidth: 80 }}>Git Tag:</span>
                      <code style={{ background: 'var(--color-bg-elevated)', padding: '2px 8px', borderRadius: 4, fontFamily: 'monospace' }}>{versionInfo.git_describe}</code>
                    </div>
                  </Space>
                </div>
                <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 16 }}>
                  <Paragraph type="secondary" style={{ margin: 0 }}>
                    NTD (Nothing Todo) 是一个 AI Todo 应用，支持 Claude Code 和 JoinAI 等多种执行器。
                  </Paragraph>
                </div>
              </div>
            ) : (
              <Empty description="无法获取版本信息" />
            )}
          </Card>
          <ShareCard />
        </Spin>
      ),
    },
  ];

  return (
    <div
      className="settings-page-root detail-panel"
      style={{
        height: '100%',
        overflowY: 'auto',
      }}
    >
      <div className="settings-header-card detail-card header-card">
        {onBack && (
          <Button
            type="text"
            size="small"
            icon={<LeftOutlined />}
            onClick={onBack}
            className="settings-back-btn"
            aria-label="返回"
          />
        )}
        <div style={{ minWidth: 0 }}>
          <h2 className="card-title">配置管理</h2>
        </div>
      </div>
      <Tabs className="settings-tabs" items={tabItems} type="card" size="small" />

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
          dataSource={state.todos}
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

      <Modal
        title={execDetailRecord ? `执行记录 #${execDetailRecord.id}` : '执行记录'}
        open={!!execDetailRecord}
        onCancel={() => setExecDetailRecord(null)}
        footer={null}
        width={700}
      >
        {execDetailRecord && (
          <div style={{ maxHeight: '60vh', overflow: 'auto' }}>
            <div style={{ display: 'flex', gap: 16, marginBottom: 12, flexWrap: 'wrap' }}>
              <span><strong>状态:</strong> {execDetailRecord.status}</span>
              <span><strong>执行器:</strong> {execDetailRecord.executor || '-'}</span>
              <span><strong>触发:</strong> {execDetailRecord.trigger_type}</span>
              {execDetailRecord.model && <span><strong>模型:</strong> {execDetailRecord.model}</span>}
            </div>
            <div style={{ marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>
              开始: {execDetailRecord.started_at ? new Date(execDetailRecord.started_at).toLocaleString() : '-'}
              {execDetailRecord.finished_at && ` | 结束: ${new Date(execDetailRecord.finished_at).toLocaleString()}`}
            </div>
            {execDetailRecord.result && (
              <div style={{ marginBottom: 12 }}>
                <strong>结果:</strong>
                <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4 }}>
                  {execDetailRecord.result}
                </pre>
              </div>
            )}
            {execDetailRecord.stdout && (
              <div style={{ marginBottom: 12 }}>
                <strong>输出:</strong>
                <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4 }}>
                  {execDetailRecord.stdout}
                </pre>
              </div>
            )}
            {execDetailRecord.stderr && (
              <div>
                <strong>错误:</strong>
                <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 150, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4, color: 'var(--color-error)' }}>
                  {execDetailRecord.stderr}
                </pre>
              </div>
            )}
          </div>
        )}
      </Modal>
    </div>
  );
}
