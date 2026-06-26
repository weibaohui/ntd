import { useState, useEffect } from 'react';
import { Spin, Empty, Space, Typography, Button, Alert, Modal, Collapse, message } from 'antd';
import { ExclamationCircleFilled, ReloadOutlined, CloudDownloadOutlined, CopyOutlined, CodeOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import { ShareCard } from '@/components/ShareCard';
import { copyToClipboard } from '@/utils/clipboard';

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
  { step: '步骤 1', label: '升级 npm 包', code: 'npm install -g @weibaohui/nothing-todo@latest' },
  { step: '步骤 2', label: '自动重新部署服务', code: 'ntd daemon install --force && ntd daemon start' },
];

// 手动升级使用的命令列表（与一键升级步骤一致，但命令更通用）。
// 用于展示给用户复制到 AI Coding 工具中手动执行。
// 与 UPGRADE_STEPS 的区别：步骤 2 使用 uninstall + install + restart
// 便于用户在终端中独立执行，不依赖后端 fork 子进程的自动流程。
const MANUAL_UPGRADE_STEPS = [
  { label: '1. 升级 npm 包', code: 'npm install -g @weibaohui/nothing-todo@latest' },
  { label: '2. 重新注册并重启服务', code: 'ntd daemon uninstall && ntd daemon install && ntd daemon restart' },
];

// 将手动升级的全部命令合并为一段脚本，方便用户一次性复制给 AI 工具。
// 使用 && 串联：前一步失败则停止，避免部分执行导致状态不一致。
const MANUAL_UPGRADE_SCRIPT = MANUAL_UPGRADE_STEPS.map(s => s.code).join(' && ');

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

  useEffect(() => {
    setVersionLoading(true);
    db.getVersion()
      .then((info) => {
        setVersionInfo(info);
      })
      .catch(() => {})
      .finally(() => setVersionLoading(false));
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

              {/* 手动升级折叠面板（Issue #581）：在版本检查有结果后展示，
                  供无法通过一键升级完成更新的用户（如 Docker 部署、无 systemd 环境），
                  将命令复制到 Claude / Pi 等 AI Coding 工具中手动执行。 */}
              {versionStatus && (
                <div style={{ marginTop: 16 }}>
                  <Collapse
                    ghost
                    size="small"
                    items={[
                      {
                        key: 'manual-upgrade',
                        label: (
                          <Space size={4}>
                            <CodeOutlined />
                            <span>手动升级（复制命令到 AI 工具中执行）</span>
                          </Space>
                        ),
                        children: (
                          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                            {/* 提示文案：说明手动升级的适用场景，引导用户复制命令 */}
                            <Alert
                              type="info"
                              message={
                                <Typography.Text style={{ fontSize: 13, lineHeight: 1.6 }}>
                                  如果一键更新不适用于您的环境（例如 Docker 部署），请将以下命令复制并粘贴到{' '}
                                  <Typography.Text code style={{ fontSize: 13 }}>Claude Code</Typography.Text>
                                  {' '}、{' '}
                                  <Typography.Text code style={{ fontSize: 13 }}>Pi</Typography.Text>
                                  {' '}或其他 AI Coding 工具中执行。
                                  命令将依次执行 npm 包升级和服务重启。
                                </Typography.Text>
                              }
                              showIcon
                              style={{ marginBottom: 4 }}
                            />

                            {/* 逐条展示手动升级命令，每条附带独立的复制按钮，
                                让用户可以选择只复制某一步的命令。 */}
                            {MANUAL_UPGRADE_STEPS.map((step) => (
                              <div
                                key={step.label}
                                style={{
                                  display: 'flex',
                                  alignItems: 'center',
                                  gap: 8,
                                }}
                              >
                                <Typography.Text
                                  style={{
                                    minWidth: 160,
                                    fontSize: 13,
                                    color: 'var(--color-text-secondary)',
                                    flexShrink: 0,
                                  }}
                                >
                                  {step.label}
                                </Typography.Text>
                                <code
                                  style={{
                                    flex: 1,
                                    background: 'var(--color-bg-elevated)',
                                    padding: '8px 12px',
                                    borderRadius: 6,
                                    fontFamily: 'monospace',
                                    fontSize: 13,
                                    lineHeight: 1.5,
                                    overflowX: 'auto',
                                    whiteSpace: 'nowrap',
                                  }}
                                >
                                  {step.code}
                                </code>
                                <Button
                                  type="text"
                                  size="small"
                                  icon={<CopyOutlined />}
                                  onClick={async () => {
                                    // 使用统一的复制工具（兼容 HTTP 环境）
                                    const ok = await copyToClipboard(step.code);
                                    if (ok) {
                                      message.success(`已复制：${step.label}`);
                                    } else {
                                      message.error('复制失败');
                                    }
                                  }}
                                />
                              </div>
                            ))}

                            {/* 分隔线 + 一键复制全部命令的按钮 */}
                            <div
                              style={{
                                borderTop: '1px solid var(--color-border-light)',
                                paddingTop: 8,
                                display: 'flex',
                                justifyContent: 'flex-end',
                              }}
                            >
                              <Button
                                size="small"
                                icon={<CopyOutlined />}
                                onClick={async () => {
                                  // 使用统一的复制工具（兼容 HTTP 环境）
                                  const ok = await copyToClipboard(MANUAL_UPGRADE_SCRIPT);
                                  if (ok) {
                                    message.success('已复制全部升级命令');
                                  } else {
                                    message.error('复制失败');
                                  }
                                }}
                              >
                                复制全部命令
                              </Button>
                            </div>
                          </div>
                        ),
                      },
                    ]}
                  />
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
                NTD (Nothing Todo) 是一个 AI Todo 应用，支持 Claude Code 和 MobileCoder 等多种执行器。
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
