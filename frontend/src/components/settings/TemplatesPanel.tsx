// 模板管理面板
//
// 合并三类模板管理：
// 1. 专家模板 - 从专家管理页面复用
// 2. 事项模板 - 从原「事项模板」管理复用
// 3. Skill 模板 - 从 bundled/skills 目录扫描加载
//
// 在 Tab 之间切换。
// 顶部「远程仓库」配置区统一管理：
// - 远程仓库地址
// - 同步策略
// - 自动同步
// - 同步全部（experts + todos + skills）

import { useState, useEffect, useCallback } from 'react';
import {
  App,
  Button,
  Empty,
  Form,
  Input,
  Modal,
  Space,
  Switch,
  Tabs,
  Tag,
  Tooltip,
  Descriptions,
  message as antMessage,
} from 'antd';
import {
  CloudDownloadOutlined,
  ReloadOutlined,
  SettingOutlined,
  CheckCircleOutlined,
  ExclamationCircleOutlined,
  GithubOutlined,
} from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CronPresetSelect } from '@/components/CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '@/utils/cron';
import { ExpertsTemplatesTab } from './templates/ExpertsTemplatesTab';
import { TodoTemplatesTab } from './templates/TodoTemplatesTab';
import { SkillTemplatesTab } from './templates/SkillTemplatesTab';
import { bundledApi } from '@/api/bundled';

/**
 * 模板管理面板入口
 *
 * 功能：
 * - Tab 1：专家模板 - 复用 ExpertsPanel
 * - Tab 2：事项模板 - TodoTemplatesTab
 * - 顶部：远程仓库配置（共用一个 git 仓库地址）
 */
export function TemplatesPanel() {
  const { message } = App.useApp();
  const [activeTab, setActiveTab] = useState<'experts' | 'todos' | 'skills'>('experts');
  const [configModalOpen, setConfigModalOpen] = useState(false);
  const [statusModalOpen, setStatusModalOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [status, setStatus] = useState<any>(null);
  const [config, setConfig] = useState<any>(null);

  /**
   * 加载同步状态
   */
  const loadStatus = useCallback(async () => {
    try {
      const res = await bundledApi.getStatus('all');
      setStatus(res);
    } catch {
      // 静默失败
    }
  }, []);

  /**
   * 加载配置
   */
  const loadConfig = useCallback(async () => {
    try {
      const res = await bundledApi.getConfig();
      setConfig(res);
    } catch {
      // 静默失败
    }
  }, []);

  useEffect(() => {
    loadStatus();
    loadConfig();
  }, [loadStatus, loadConfig]);

  /**
   * 触发同步（同步全部资源：experts + todos + skills）
   */
  const handleSync = async () => {
    setSyncing(true);
    const hide = antMessage.loading('正在同步全部资源...', 0);
    try {
      const res = await bundledApi.sync({ subdir: 'all', strategy: 'overwrite' });
      if (res?.success) {
        message.success(`同步成功: ${res.message}`);
        await loadStatus();
      } else {
        message.warning(res?.message || '同步未完成');
      }
    } catch (e: any) {
      message.error(`同步失败: ${e.message || e}`);
    } finally {
      hide();
      setSyncing(false);
    }
  };

  return (
    <div className="ntd-templates-panel">
      <Space style={{ marginBottom: 16 }}>
        <Tooltip title="查看同步状态">
          <Button
            icon={<CloudDownloadOutlined />}
            onClick={() => setStatusModalOpen(true)}
          >
            同步状态
          </Button>
        </Tooltip>
        <Tooltip title="远程仓库配置">
          <Button
            icon={<SettingOutlined />}
            onClick={async () => {
              await loadConfig();
              setConfigModalOpen(true);
            }}
          >
            仓库配置
          </Button>
        </Tooltip>
        <Tooltip title="立即同步全部资源">
          <Button
            type="primary"
            icon={<ReloadOutlined spin={syncing} />}
            loading={syncing}
            onClick={handleSync}
          >
            立即同步
          </Button>
        </Tooltip>
      </Space>

      <Tabs
        activeKey={activeTab}
        onChange={(k) => setActiveTab(k as 'experts' | 'todos' | 'skills')}
        items={[
          {
            key: 'experts',
            label: (
              <Space>
                <span>专家模板</span>
                <SyncBadge
                  fileCount={status?.subdir === 'experts' ? status?.subdir_file_count : undefined}
                  needsUpdate={status?.subdir === 'experts' ? status?.needs_update : undefined}
                />
              </Space>
            ),
            children: (
              <ExpertsTabContent />
            ),
          },
          {
            key: 'todos',
            label: (
              <Space>
                <span>事项模板</span>
                <SyncBadge
                  fileCount={status?.subdir === 'todos' ? status?.subdir_file_count : undefined}
                  needsUpdate={status?.subdir === 'todos' ? status?.needs_update : undefined}
                />
              </Space>
            ),
            children: (
              <TodoTemplatesTab />
            ),
          },
          {
            key: 'skills',
            label: (
              <Space>
                <span>Skill 模板</span>
                <SyncBadge
                  fileCount={status?.subdir === 'skills' ? status?.subdir_file_count : undefined}
                  needsUpdate={status?.subdir === 'skills' ? status?.needs_update : undefined}
                />
              </Space>
            ),
            children: (
              <SkillTemplatesTab />
            ),
          },
        ]}
      />

      <ConfigModal
        open={configModalOpen}
        config={config}
        onClose={() => setConfigModalOpen(false)}
        onSaved={async () => {
          setConfigModalOpen(false);
          await loadConfig();
          await loadStatus();
          message.success('配置已保存');
        }}
      />

      <StatusModal
        open={statusModalOpen}
        status={status}
        onClose={() => setStatusModalOpen(false)}
        onSync={async () => {
          setStatusModalOpen(false);
          await handleSync();
        }}
        onRefresh={loadStatus}
      />
    </div>
  );
}

/**
 * 同步状态徽标
 */
function SyncBadge({
  fileCount,
  needsUpdate,
}: {
  fileCount?: number;
  needsUpdate?: boolean;
}) {
  if (fileCount === undefined) return null;
  return (
    <Tag color={needsUpdate ? 'orange' : 'green'} style={{ marginLeft: 4 }}>
      {fileCount}
    </Tag>
  );
}

/**
 * 专家 Tab 内容
 */
function ExpertsTabContent() {
  return <ExpertsTemplatesTab />;
}

/**
 * 配置弹窗
 */
function ConfigModal({
  open,
  config,
  onClose,
  onSaved,
}: {
  open: boolean;
  config: any;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [form] = Form.useForm();
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (open && config) {
      form.setFieldsValue({
          url: config.url,
          branch: config.branch,
          auto_sync_enabled: config.auto_sync_enabled,
          auto_sync_cron: config.auto_sync_cron,
        });
    }
  }, [open, config, form]);

  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      await bundledApi.updateConfig(values);
      onSaved();
    } catch (e: any) {
      antMessage.error(`保存失败: ${e.message || e}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title="远程仓库配置"
      open={open}
      onCancel={onClose}
      onOk={handleSave}
      confirmLoading={saving}
      width={600}
    >
      <Form form={form} layout="vertical" style={{ marginTop: 16 }}>
        <Form.Item
          name="url"
          label="远程仓库地址"
          rules={[{ required: true, message: '请输入远程仓库地址' }]}
        >
          <Input prefix={<GithubOutlined />} placeholder="https://gitcode.com/..." />
        </Form.Item>
        <Form.Item name="branch" label="目标分支">
          <Input placeholder="main" />
        </Form.Item>
        <Form.Item name="auto_sync_enabled" label="启用自动同步" valuePropName="checked">
          <Switch />
        </Form.Item>
        <Form.Item
          noStyle
          shouldUpdate={(prevValues, curValues) => prevValues.auto_sync_enabled !== curValues.auto_sync_enabled}
        >
          {({ getFieldValue }) => {
            const enabled = getFieldValue('auto_sync_enabled');
            if (!enabled) return null;
            return (
              <Form.Item name="auto_sync_cron" label="自动同步周期">
                <CronPresetSelect
                  value={form.getFieldValue('auto_sync_cron') || ''}
                  onChange={(val) => form.setFieldValue('auto_sync_cron', val)}
                />
                <div style={{ marginTop: 12 }}>
                  <Cron
                    value={cronTo5(form.getFieldValue('auto_sync_cron') || '0 4 * * *')}
                    setValue={(val: string) => form.setFieldValue('auto_sync_cron', cronTo6(val))}
                    locale={CRON_ZH_LOCALE}
                    defaultPeriod="hour"
                    humanizeLabels
                    allowClear={false}
                  />
                </div>
              </Form.Item>
            );
          }}
        </Form.Item>
      </Form>
    </Modal>
  );
}

/**
 * 状态弹窗
 */
function StatusModal({
  open,
  status,
  onClose,
  onSync,
  onRefresh,
}: {
  open: boolean;
  status: any;
  onClose: () => void;
  onSync: () => void;
  onRefresh: () => void;
}) {
  return (
    <Modal
      title="同步状态"
      open={open}
      onCancel={onClose}
      footer={[
        <Button key="refresh" icon={<ReloadOutlined />} onClick={onRefresh}>
          刷新
        </Button>,
        <Button key="all" type="primary" onClick={onSync}>
          同步全部
        </Button>,
      ]}
      width={600}
    >
      {status ? (
        <Descriptions column={1} bordered size="small">
          <Descriptions.Item label="远程仓库">{status.remote_url}</Descriptions.Item>
          <Descriptions.Item label="分支">{status.branch}</Descriptions.Item>
          <Descriptions.Item label="本地路径">{status.local_path}</Descriptions.Item>
          <Descriptions.Item label="同步策略">{status.sync_strategy}</Descriptions.Item>
          <Descriptions.Item label="自动同步">
            {status.auto_sync_enabled ? '已启用' : '未启用'}
          </Descriptions.Item>
          <Descriptions.Item label="本地仓库">
            {status.local_exists ? (
              <Tag color="green" icon={<CheckCircleOutlined />}>
                已就绪
              </Tag>
            ) : (
              <Tag color="orange" icon={<ExclamationCircleOutlined />}>
                未初始化
              </Tag>
            )}
          </Descriptions.Item>
          <Descriptions.Item label="本地 Commit">
            {status.local_commit ? status.local_commit.substring(0, 8) : '-'}
          </Descriptions.Item>
          <Descriptions.Item label="远程 Commit">
            {status.remote_commit ? status.remote_commit.substring(0, 8) : '-'}
          </Descriptions.Item>
          <Descriptions.Item label="是否需要更新">
            {status.needs_update ? (
              <Tag color="orange">有更新</Tag>
            ) : status.needs_update === false ? (
              <Tag color="green">最新</Tag>
            ) : (
              '-'
            )}
          </Descriptions.Item>
          <Descriptions.Item label="上次同步时间">
            {status.last_sync_at || '从未同步'}
          </Descriptions.Item>
        </Descriptions>
      ) : (
        <Empty description="加载状态失败" />
      )}
    </Modal>
  );
}
