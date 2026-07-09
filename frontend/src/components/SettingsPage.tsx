import { useState, useEffect, useMemo, useCallback } from 'react';
import { Tabs, Form, message } from 'antd';
import { useIsMobile } from '@/hooks/useIsMobile';
import {
  SettingOutlined,
  TagOutlined,
  SaveOutlined,
  FileTextOutlined,
  InfoCircleOutlined,
  CloudOutlined,
  DesktopOutlined,
} from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { useApp } from '@/hooks/useApp';
import { useViewState } from '@/hooks/useViewState';
import * as db from '@/utils/database';
import type { Config, SlashCommandRule } from '@/types';
import { SystemSettingsPanel } from './settings/SystemSettingsPanel';
import { TagsPanel } from './settings/TagsPanel';
import { BackupPanel } from './settings/BackupPanel';
import { TemplatesPanel } from './settings/TemplatesPanel';
import { AboutPanel } from './settings/AboutPanel';
import { CloudSyncPanel } from './settings/CloudSyncPanel';
import { InterfaceDisplayPanel } from './settings/InterfaceDisplayPanel';

import { DEFAULT_EXECUTION_TIMEOUT_SECS } from '@/constants';

/** 设置页，负责加载并保存系统配置以及各类管理面板。 */
export function SettingsPage() {
  const { state, dispatch } = useApp();
  const { tags } = state;
  const isMobile = useIsMobile();

  const [configForm] = Form.useForm();
  const [configLoading, setConfigLoading] = useState(false);
  const [configSaving, setConfigSaving] = useState(false);

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
  // 1. 系统设置、界面显示、标签管理 → 基础配置优先
  // 2. 事项模板 → 项目相关
  // 3. 备份与恢复 → 数据安全
  // 4. 云端同步 → 外部集成
  // 5. 关于 → 信息页末位
  //
  // 执行器管理、会话管理、工作空间、Skills 管理、运行管理已独立为左侧导航菜单项，
  // 不再嵌套在设置页的标签页中。
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
      key: 'interface',
      label: <span><DesktopOutlined style={{ marginRight: 6 }} />界面显示</span>,
      children: <InterfaceDisplayPanel />,
    },
    {
      key: 'tags',
      label: <span><TagOutlined style={{ marginRight: 6 }} />标签管理</span>,
      children: <TagsPanel tags={tags} dispatch={dispatch} />,
    },
    {
      key: 'templates',
      label: <span><FileTextOutlined style={{ marginRight: 6 }} />事项模板</span>,
      children: <TemplatesPanel />,
    },
    {
      key: 'backup',
      label: <span><SaveOutlined style={{ marginRight: 6 }} />备份与恢复</span>,
      children: <BackupPanel />,
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

  const knownTabs = useMemo(() => tabItems.map(t => t.key), [tabItems]);

  // 从 useViewState 获取当前 tab，popstate 由它统一处理
  const { activeTab, pushUrl } = useViewState();
  const resolvedTab = activeTab && knownTabs.includes(activeTab) ? activeTab : 'system';

  const handleTabChange = useCallback((key: string) => {
    pushUrl('settings', { tab: key });
  }, [pushUrl]);

  return (
    <PageCard
      icon={<SettingOutlined />}
      title="设置"
    >
      <Tabs
        className="settings-tabs"
        items={tabItems}
        type="card"
        size="small"
        tabPosition={isMobile ? 'top' : 'left'}
        activeKey={resolvedTab}
        onChange={handleTabChange}
      />
    </PageCard>
  );
}
