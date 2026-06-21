import { useState, useEffect, useMemo } from 'react';
import { Tabs, Form, Button, message } from 'antd';
import {
  SettingOutlined,
  CodeOutlined,
  TagOutlined,
  SaveOutlined,
  FolderOutlined,
  ThunderboltOutlined,
  InfoCircleOutlined,
  MessageOutlined,
  PlayCircleOutlined,
  LaptopOutlined,
  FileTextOutlined,
  LeftOutlined,
  ApiOutlined,
  CloudOutlined,
  AuditOutlined,
} from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import * as db from '@/utils/database';
import type { Config, ExecutorConfig, SlashCommandRule } from '@/types';
import { SkillsPanel } from './SkillsPanel';
import { SessionManager } from './SessionManager';
import { WebhooksPanel } from './WebhooksPanel';
import { SystemSettingsPanel } from './settings/SystemSettingsPanel';
import { ExecutorsPanel } from './settings/ExecutorsPanel';
import { TagsPanel } from './settings/TagsPanel';
import { ProjectDirectoriesPanel } from './settings/ProjectDirectoriesPanel';
import { BackupPanel } from './settings/BackupPanel';
import { RuntimePanel } from './settings/RuntimePanel';
import { MessagesPanel } from './settings/MessagesPanel';
import { TemplatesPanel } from './settings/TemplatesPanel';
import { AboutPanel } from './settings/AboutPanel';
import { CloudSyncPanel } from './settings/CloudSyncPanel';
import { ReviewTemplatesPanel } from './settings/ReviewTemplatesPanel';

import { DEFAULT_EXECUTION_TIMEOUT_SECS } from '@/constants';

interface SettingsPageProps {
  onBack?: () => void;
}

/** 设置页，负责加载并保存系统配置以及各类管理面板。 */
export function SettingsPage({ onBack }: SettingsPageProps) {
  const { state, dispatch } = useApp();
  const { tags } = state;

  const [configForm] = Form.useForm();
  const [configLoading, setConfigLoading] = useState(false);
  const [configSaving, setConfigSaving] = useState(false);

  // Executors state (shared between ExecutorsPanel and RuntimePanel)
  const [executors, setExecutors] = useState<ExecutorConfig[]>([]);
  const [executorsLoading, setExecutorsLoading] = useState(false);

  const executorDisplayNames = useMemo(() => {
    const map: Record<string, string> = {};
    for (const ec of executors) {
      map[ec.name] = ec.display_name;
    }
    return map;
  }, [executors]);

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

  /** 汇总表单值并保存当前系统配置。 */
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
        execution_timeout_secs: configForm.getFieldValue('execution_timeout_secs') ?? currentConfig.execution_timeout_secs ?? DEFAULT_EXECUTION_TIMEOUT_SECS,
        slash_command_rules: hasSlashRulesField
          ? normalizedRules
          : (currentConfig.slash_command_rules ?? []),
      };

      setConfigSaving(true);
      await db.updateConfig(mergedConfig);
      configForm.setFieldsValue(mergedConfig);
      message.success('配置已保存');
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setConfigSaving(false);
    }
  };

  // Tab 顺序说明：
  // 1. 系统设置、执行器管理、标签管理 → 基础配置优先
  // 2. 消息、Session 管理 → 用户个性化配置紧随其后
  // 3. 项目目录、模板管理 → 项目相关
  // 4. 备份与恢复 → 数据安全（用户配置完毕后最后考虑）
  // 5. Skills 管理、运行管理 → 高级功能放中间层
  // 6. Webhook、云端同步 → 外部集成邻近放置
  // 7. 关于 → 信息页末位
  const tabItems = [
    {
      key: 'system',
      label: <span><SettingOutlined style={{ marginRight: 6 }} />系统设置</span>,
      children: (
        <SystemSettingsPanel
          configForm={configForm}
          configSaving={configSaving}
          configLoading={configLoading}
          handleSaveConfig={handleSaveConfig}
        />
      ),
    },
    {
      key: 'executors',
      label: <span><CodeOutlined style={{ marginRight: 6 }} />执行器管理</span>,
      children: (
        <ExecutorsPanel
          executors={executors}
          setExecutors={setExecutors}
          executorsLoading={executorsLoading}
        />
      ),
    },
    {
      key: 'tags',
      label: <span><TagOutlined style={{ marginRight: 6 }} />标签管理</span>,
      children: <TagsPanel tags={tags} dispatch={dispatch} />,
    },
    {
      key: 'messages',
      label: <span><MessageOutlined style={{ marginRight: 6 }} />消息</span>,
      children: (
        <MessagesPanel
          configForm={configForm}
          configSaving={configSaving}
          handleSaveConfig={handleSaveConfig}
          onBack={onBack}
        />
      ),
    },
    {
      key: 'sessions',
      label: <span><LaptopOutlined style={{ marginRight: 6 }} />Session 管理</span>,
      children: <SessionManager />,
    },
    {
      key: 'projectDirectories',
      label: <span><FolderOutlined style={{ marginRight: 6 }} />工作空间</span>,
      children: <ProjectDirectoriesPanel />,
    },
    {
      key: 'templates',
      label: <span><FileTextOutlined style={{ marginRight: 6 }} />模板管理</span>,
      children: <TemplatesPanel />,
    },
    {
      key: 'reviewTemplates',
      label: <span><AuditOutlined style={{ marginRight: 6 }} />评审模板</span>,
      children: <ReviewTemplatesPanel />,
    },
    {
      key: 'backup',
      label: <span><SaveOutlined style={{ marginRight: 6 }} />备份与恢复</span>,
      children: <BackupPanel />,
    },
    {
      key: 'skills',
      label: <span><ThunderboltOutlined style={{ marginRight: 6 }} />Skills 管理</span>,
      children: <SkillsPanel />,
    },
    {
      key: 'runtime',
      label: <span><PlayCircleOutlined style={{ marginRight: 6 }} />运行管理</span>,
      children: (
        <RuntimePanel
          configForm={configForm}
          configSaving={configSaving}
          handleSaveConfig={handleSaveConfig}
          executorDisplayNames={executorDisplayNames}
        />
      ),
    },
    {
      key: 'webhooks',
      label: <span><ApiOutlined style={{ marginRight: 6 }} />Webhook</span>,
      children: <WebhooksPanel todos={state.todos} />,
    },
    {
      key: 'cloudSync',
      label: <span><CloudOutlined style={{ marginRight: 6 }} />云端同步</span>,
      children: <CloudSyncPanel />,
    },
    {
      key: 'about',
      label: <span><InfoCircleOutlined style={{ marginRight: 6 }} />关于</span>,
      children: <AboutPanel />,
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
    </div>
  );
}
