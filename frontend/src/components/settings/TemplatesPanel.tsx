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
  Alert,
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
import { InstallGitButton } from './InstallGitButton';

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
  // 同步成功后递增，作为传给 SkillTemplatesTab 的刷新信号；tab 自行据此重拉列表，避免列表陈旧。
  const [skillRefreshTick, setSkillRefreshTick] = useState(0);
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
    // 前置依赖检查：git 没装时同步注定失败（后端靠系统 git CLI 拉仓库），
    // 干脆别发那个注定 500 的请求，直接引导用户去装 git，体验远好于事后看一条笼统错误。
    if (status && !status.git_available) {
      message.warning('未检测到 Git，请先安装 Git 后再同步');
      // 弹出状态面板：里面「Git 运行环境」那行带「安装 Git」按钮，指给用户一条明路
      setStatusModalOpen(true);
      return;
    }
    setSyncing(true);
    const hide = antMessage.loading('正在同步全部资源...', 0);
    try {
      const res = await bundledApi.sync({ subdir: 'all', strategy: 'overwrite' });
      if (res?.success) {
        message.success(`同步成功: ${res.message}`);
        await loadStatus();
        // skills 也被 subdir 'all' 一起同步了，递增 tick 让 Skill 模板 Tab 重拉，避免列表陈旧。
        setSkillRefreshTick((t) => t + 1);
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
      {/* git 是 bundled 资源同步的硬前置依赖：缺失时在面板顶部强提示 + 内联一键安装入口，
          而不是等用户点了「立即同步」再去吃一个笼统的 500。装完 onInstalled 触发 loadStatus 重探，
          status.git_available 变 true 后本横幅自动消失。 */}
      {status && !status.git_available && (
        <Alert
          showIcon
          type="error"
          style={{ marginBottom: 16 }}
          message="未检测到 Git"
          description="远程仓库同步依赖系统 Git，当前环境未检测到。点右侧「安装 Git」一键安装；装完在弹窗里点「应用」即可重新检测。若已装好仍显示未安装，说明后端进程的 PATH 是启动时固定的、未刷新，重启本应用后即恢复。"
          action={<InstallGitButton onInstalled={loadStatus} buttonType="primary" showLabel />}
        />
      )}
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
              <SkillTemplatesTab refreshTick={skillRefreshTick} />
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
          {/* Git 运行环境放第一行：这是同步能否进行的最关键前置条件，
              缺失时直接在行内给「安装 Git」按钮（onRefresh 重探后标签转绿）。 */}
          <Descriptions.Item label="Git 运行环境">
            {status.git_available ? (
              <Tag color="green" icon={<CheckCircleOutlined />}>
                已安装
              </Tag>
            ) : (
              <Space>
                <Tag color="red" icon={<ExclamationCircleOutlined />}>
                  未安装
                </Tag>
                <InstallGitButton onInstalled={onRefresh} buttonSize="small" />
              </Space>
            )}
          </Descriptions.Item>
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
