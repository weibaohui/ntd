import { useState, useEffect } from 'react';
import { Spin, Empty, Space, Typography, Button, Alert, Modal, message, Switch, Select } from 'antd';
import { ExclamationCircleFilled, ReloadOutlined, CloudDownloadOutlined, SettingOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import { ShareCard } from '@/components/ShareCard';
import { ManualUpgradeButton } from '@/components/settings/ManualUpgradeButton';

const { Paragraph } = Typography;

interface VersionStatus {
  current: string;
  latest: string | null;
  isUpToDate: boolean | null; // null = 还没检查
  error?: string;
}

// 分离式自更新方案 (issue #569) 不再需要 UpgradeResult 类型，
// 后端先返回成功响应，再 exit(0) 后前端通过自动刷新访问新版本服务。

// 分离式自更新方案 (issue #569)：升级时后端会先返回 HTTP 成功响应，
// 然后 spawn 后台任务延迟 500ms 后 exit(0)。前端主要依赖成功响应，
// 5s 后自动刷新页面访问新版本服务。极端情况 exit(0) 过早导致 TCP 断开，
// catch 兜底后仍由 5s 定时器刷新。
const UPGRADE_RELOAD_DELAY_MS = 5000;

// 分离式自更新方案 (issue #569)：后端升级后主进程 exit(0)，
// 子进程在后台 sleep 3s 后执行 install --force + start。
// 因此前端看到的步骤简化为两步，弹窗描述更贴近用户感知。
const UPGRADE_STEPS = [
  { step: '步骤 1', label: '升级 npm 包', code: 'npm install -g @weibaohui/ntd@latest' },
  { step: '步骤 2', label: '自动重新部署服务', code: 'ntd daemon install --force && ntd daemon start' },
];

/**
 * 渲染更新确认弹窗的内容区域。
 */
function renderUpgradeConfirmContent() {
  return (
    <div className="update-confirm-modal__content">
      <div className="update-confirm-modal__hero">
        <div className="update-confirm-modal__eyebrow">桌面端一键更新</div>
        <div className="update-confirm-modal__hero-text">
          将执行以下操作完成更新：升级 npm 包后自动重新部署服务。
        </div>
      </div>

      <div className="update-confirm-modal__command-list">
        {UPGRADE_STEPS.map(({ step, label, code }) => (
          <div key={step} className="update-confirm-modal__command-card">
            <div className="update-confirm-modal__command-header">
              <span className="update-confirm-modal__command-step">{step}</span>
              <span className="update-confirm-modal__command-label">{label}</span>
            </div>
            <code className="update-confirm-modal__command-code">
              {code}
            </code>
          </div>
        ))}
      </div>

      <Paragraph className="update-confirm-modal__note" type="secondary">
        升级过程中页面会自动刷新，请耐心等待。
      </Paragraph>
    </div>
  );
}

export function AboutPanel() {
  const [versionInfo, setVersionInfo] = useState<{ version: string; git_sha: string; git_describe: string } | null>(null);
  const [versionLoading, setVersionLoading] = useState(false);
  const [versionStatus, setVersionStatus] = useState<VersionStatus | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [upgrading, setUpgrading] = useState(false);
  // upgradeResult 相关状态已移除（分离式自更新方案，后端 exit(0) 后无法返回响应）。
  // 升级结果由定时刷新自动处理。

  // 自动更新配置状态
  const [autoUpdateEnabled, setAutoUpdateEnabled] = useState(false);
  const [autoUpdateInterval, setAutoUpdateInterval] = useState<string>('day');
  const [autoUpdateHour, setAutoUpdateHour] = useState<number>(3);
  const [autoUpdateLastCheck, setAutoUpdateLastCheck] = useState<string | null>(null);
  const [autoUpdateLoading, setAutoUpdateLoading] = useState(false);

  useEffect(() => {
    setVersionLoading(true);
    db.getVersion()
      .then((info) => {
        setVersionInfo(info);
      })
      .catch(() => {})
      .finally(() => setVersionLoading(false));

    // 加载自动更新配置
    db.getAutoUpdateSettings()
      .then((settings) => {
        setAutoUpdateEnabled(settings.auto_update_enabled);
        setAutoUpdateInterval(settings.auto_update_interval);
        setAutoUpdateHour(settings.auto_update_hour);
        setAutoUpdateLastCheck(settings.auto_update_last_check_at);
      })
      .catch(() => {});
  }, []);

  // 规范化版本号：去除 v 前缀、-dirty/-alpha 等后缀，只保留 semver 主干
  // 例如 "v0.0.50-dirty" -> "0.0.50"，"0.0.50-alpha" -> "0.0.50"
  const normalizeVersion = (v: string): string => {
    return v.replace(/^v/, '').replace(/-.+$/, '').trim();
  };

  // 版本比较函数：比较两个版本号，返回 1 表示 a 更新，-1 表示 b 更新，0 表示相等
  const compareVersions = (a: string, b: string): number => {
    const normA = normalizeVersion(a);
    const normB = normalizeVersion(b);
    const partsA = normA.split('.').map(Number);
    const partsB = normB.split('.').map(Number);
    for (let i = 0; i < Math.max(partsA.length, partsB.length); i++) {
      const pA = partsA[i] ?? 0;
      const pB = partsB[i] ?? 0;
      if (pA > pB) return 1;
      if (pA < pB) return -1;
    }
    return 0;
  };

  // 检查最新版本
  const checkForUpdate = async () => {
    if (!versionInfo) return;
    setCheckingUpdate(true);
    // 分离式自更新方案：不再清除旧的升级结果（已移除 upgradeResult 状态）
    try {
      const result = await db.getLatestVersion();
      if (result.latest) {
        const isUpToDate = compareVersions(versionInfo.version, result.latest) >= 0;
        setVersionStatus({
          current: versionInfo.version,
          latest: result.latest,
          isUpToDate,
        });
      } else {
        setVersionStatus({
          current: versionInfo.version,
          latest: null,
          isUpToDate: null,
          error: result.error || '无法获取最新版本',
        });
      }
    } catch (e) {
      setVersionStatus({
        current: versionInfo.version,
        latest: null,
        isUpToDate: null,
        error: '检查更新失败',
      });
    } finally {
      setCheckingUpdate(false);
    }
  };

  // 执行一键更新（分离式自更新方案，issue #569）。
  // 后端升级流程：
  // 1. npm install -g 升级 npm 包
  // 2. 写 /tmp/ntd.update 标记
  // 3. fork 子进程 sleep 3s → install --force → start → rm 标记
  // 4. 返回成功响应给前端，然后 spawn 后台任务 500ms 后 exit(0) 让出端口
  //
  // 前端感知：正常情况收到 API 成功响应（code=0），极端情况 exit(0) 过早
  // 导致 TCP 重置触发 catch。无论哪种结果，都在 UPGRADE_RELOAD_DELAY_MS 后自动刷新页面。
  const handleUpgrade = async () => {
    if (!versionStatus?.latest) return;

    // 弹出确认框，显示将要执行的命令
    Modal.confirm({
      className: 'update-confirm-modal',
      rootClassName: 'update-confirm-modal',
      width: 560,
      title: (
        <div className="update-confirm-modal__title-group">
          <div className="update-confirm-modal__title">确认执行更新</div>
          <div className="update-confirm-modal__subtitle">将按以下步骤升级当前 NTD 服务</div>
        </div>
      ),
      icon: <ExclamationCircleFilled className="update-confirm-modal__icon" />,
      content: renderUpgradeConfirmContent(),
      okText: '执行更新',
      cancelText: '取消',
      onOk: async () => {
        setUpgrading(true);
        // 后端会先返回成功响应再 exit(0)，因此正常情况 catch 不会触发。
        // 极端情况 exit(0) 过早导致 TCP RST 时 catch 兜底。
        // 都在 UPGRADE_RELOAD_DELAY_MS 后自动刷新页面访问新版本。
        // 不保存 timer 引用：页面即将刷新，无需清除。
        setTimeout(() => {
          window.location.reload();
        }, UPGRADE_RELOAD_DELAY_MS);

        try {
          // 执行升级。通常收到正常响应（后端先返回再 exit），
          // 极端情况 exit(0) 过早导致网络错误时触发 catch。
          // 延迟刷新由上面的 setTimeout 统一兜底。
          await db.upgradeVersion();
        } catch (_e) {
          // exit(0) 过早导致 TCP RST 是极端情况，
          // 不展示错误提示，静默等待定时器刷新。
        }
        // 注意：此处不设置 setUpgrading(false)，
        // 因为页面即将刷新，没必要做这个 UI 更新。
      },
    });
  };

  // 自动更新配置保存
  const handleAutoUpdateToggle = async (enabled: boolean) => {
    setAutoUpdateLoading(true);
    try {
      await db.updateAutoUpdateSettings({ auto_update_enabled: enabled });
      setAutoUpdateEnabled(enabled);
      message.success(enabled ? '已开启自动更新' : '已关闭自动更新');
    } catch {
      message.error('保存失败');
    } finally {
      setAutoUpdateLoading(false);
    }
  };

  const handleAutoUpdateIntervalChange = async (interval: string) => {
    setAutoUpdateLoading(true);
    try {
      await db.updateAutoUpdateSettings({ auto_update_interval: interval });
      setAutoUpdateInterval(interval);
    } catch {
      message.error('保存失败');
    } finally {
      setAutoUpdateLoading(false);
    }
  };

  const handleAutoUpdateHourChange = async (hour: number) => {
    setAutoUpdateLoading(true);
    try {
      await db.updateAutoUpdateSettings({ auto_update_hour: hour });
      setAutoUpdateHour(hour);
    } catch {
      message.error('保存失败');
    } finally {
      setAutoUpdateLoading(false);
    }
  };

  // 间隔选项
  const intervalOptions = [
    { value: 'day', label: '每天' },
    { value: 'week', label: '每周' },
    { value: 'month', label: '每月' },
  ];

  // 小时选项（0-23）
  const hourOptions = Array.from({ length: 24 }, (_, i) => ({
    value: i,
    label: `${String(i).padStart(2, '0')}:00`,
  }));

  return (
    <Spin spinning={versionLoading}>
      <div style={{ maxWidth: 600 }}>
        <div style={{ fontWeight: 600, marginBottom: 16, fontSize: 16 }}>NTD 版本信息</div>
        {versionInfo ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>版本号</div>
              <div style={{ fontSize: 24, fontWeight: 700, fontFamily: 'monospace' }}>{versionInfo.version}</div>
            </div>

            {/* 版本检查区域 */}
            <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 16 }}>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 8 }}>版本检查</div>
              {versionStatus && (
                <div style={{ marginBottom: 12 }}>
                  {versionStatus.error ? (
                    <Alert type="warning" message={versionStatus.error} showIcon />
                  ) : versionStatus.isUpToDate === true ? (
                    <Alert
                      type="success"
                      message={`当前已是最新版本 ${versionStatus.current}`}
                      showIcon
                    />
                  ) : versionStatus.isUpToDate === false ? (
                    <Alert
                      type="info"
                      message={
                        <Space direction="vertical" size={4}>
                          <span>发现新版本：<strong>{versionStatus.latest}</strong></span>
                          <span style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
                            当前版本：{versionInfo.version}
                          </span>
                        </Space>
                      }
                      showIcon
                    />
                  ) : null}
                </div>
              )}

              {/* 升级进行中提示 */}
              {upgrading && (
                <div style={{ marginBottom: 12 }}>
                  <Alert
                    type="info"
                    message="正在升级 npm 包并重启服务，页面将在新服务启动后自动刷新。"
                    showIcon
                  />
                </div>
              )}

              <Space wrap>
                <Button
                  icon={<ReloadOutlined />}
                  onClick={checkForUpdate}
                  loading={checkingUpdate}
                  disabled={!versionInfo}
                >
                  {versionStatus === null ? '检查更新' : '重新检查'}
                </Button>
                {/* 发现新版本时显示一键更新按钮 */}
                {versionStatus?.isUpToDate === false && (
                  <Button
                    type="primary"
                    icon={<CloudDownloadOutlined />}
                    onClick={handleUpgrade}
                    loading={upgrading}
                  >
                    一键更新到 {versionStatus.latest}
                  </Button>
                )}
              </Space>

              {/* 自动更新设置 */}
              <div style={{ marginTop: 16, borderTop: '1px solid var(--color-border-light)', paddingTop: 16 }}>
                <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 8, display: 'flex', alignItems: 'center', gap: 4 }}>
                  <SettingOutlined />
                  <span>自动更新</span>
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                    <span>启用自动检查更新</span>
                    <Switch
                      checked={autoUpdateEnabled}
                      onChange={handleAutoUpdateToggle}
                      loading={autoUpdateLoading}
                    />
                  </div>
                  {autoUpdateEnabled && (
                    <>
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                        <span>检查频率</span>
                        <Select
                          value={autoUpdateInterval}
                          onChange={handleAutoUpdateIntervalChange}
                          options={intervalOptions}
                          style={{ width: 120 }}
                          size="small"
                          disabled={autoUpdateLoading}
                        />
                      </div>
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                        <span>检查时间</span>
                        <Select
                          value={autoUpdateHour}
                          onChange={handleAutoUpdateHourChange}
                          options={hourOptions}
                          style={{ width: 120 }}
                          size="small"
                          disabled={autoUpdateLoading}
                        />
                      </div>
                      {autoUpdateLastCheck && (
                        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
                          上次检查：{new Date(autoUpdateLastCheck).toLocaleString()}
                        </div>
                      )}
                      <Alert
                        type="info"
                        message="发现新版本后，系统会在当前运行的 Todo 或 Loop 完成后自动升级。升级期间服务会短暂重启。"
                        showIcon
                        style={{ fontSize: 12 }}
                      />
                    </>
                  )}
                </div>
              </div>

              {/* 手动升级（Issue #581）：在版本检查有结果后展示，
                  用 ActionButton 走 AI 执行器自动跑升级命令，免去用户复制命令的步骤。
                  适用于一键更新不奏效的环境（如 Docker 部署、无 systemd 等）。 */}
              {versionStatus && (
                <div style={{ marginTop: 16 }}>
                  <Alert
                    type="info"
                    message={
                      <Typography.Text style={{ fontSize: 13, lineHeight: 1.6 }}>
                        如果一键更新不适用于您的环境（例如 Docker 部署），可使用「手动升级」按钮让 AI
                        在本机执行 npm 升级。完成后点 Drawer 中的「立即重启服务」让 ntd 切到新版本。
                      </Typography.Text>
                    }
                    showIcon
                    style={{ marginBottom: 8 }}
                  />
                  <ManualUpgradeButton />
                </div>
              )}
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
                NTD (Now Task, Done) 是一个 AI 驱动的任务引擎，支持 Claude Code 和 MobileCoder 等多种执行器。
              </Paragraph>
            </div>
          </div>
        ) : (
          <Empty description="无法获取版本信息" />
        )}
        <div style={{ marginTop: 24 }}>
          <ShareCard />
        </div>
      </div>
    </Spin>
  );
}
