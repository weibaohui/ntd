import { useState, useEffect } from 'react';
import { Spin, Card, Empty, Space, Typography, Button, Alert, Modal, message } from 'antd';
import { CheckCircleFilled, CloseCircleFilled, ReloadOutlined, CloudDownloadOutlined, ExclamationCircleFilled } from '@ant-design/icons';
import * as db from '../../utils/database';
import { ShareCard } from '../ShareCard';

const { Paragraph } = Typography;

interface VersionStatus {
  current: string;
  latest: string | null;
  isUpToDate: boolean | null; // null = 还没检查
  error?: string;
}

interface UpgradeResult {
  upgraded: boolean;
  restarted: boolean;
  npmOutput?: string;
  restartMessage?: string;
}

/**
 * 渲染更新确认弹窗的内容区域。
 */
function renderUpgradeConfirmContent() {
  return (
    <div className="update-confirm-modal__content">
      <div className="update-confirm-modal__hero">
        <div className="update-confirm-modal__eyebrow">桌面端一键更新</div>
        <div className="update-confirm-modal__hero-text">
          将执行以下命令完成更新，并在结束后自动重启服务。
        </div>
      </div>

      <div className="update-confirm-modal__command-list">
        <div className="update-confirm-modal__command-card">
          <div className="update-confirm-modal__command-header">
            <span className="update-confirm-modal__command-step">步骤 1</span>
            <span className="update-confirm-modal__command-label">升级 npm 包</span>
          </div>
          <code className="update-confirm-modal__command-code">
            npm install -g @weibaohui/nothing-todo@latest
          </code>
        </div>

        <div className="update-confirm-modal__command-card">
          <div className="update-confirm-modal__command-header">
            <span className="update-confirm-modal__command-step">步骤 2</span>
            <span className="update-confirm-modal__command-label">重启服务</span>
          </div>
          <code className="update-confirm-modal__command-code">
            ntd daemon restart
          </code>
        </div>
      </div>

      <Paragraph className="update-confirm-modal__note" type="secondary">
        更新完成后服务将自动重启，请稍后刷新页面。
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
  const [upgradeResult, setUpgradeResult] = useState<UpgradeResult | null>(null);

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
    setUpgradeResult(null); // 清除之前的升级结果
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

  // 执行一键更新
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
        try {
          const result = await db.upgradeVersion();
          setUpgradeResult(result);
          if (result.upgraded && result.restarted) {
            message.success('更新命令已执行，服务正在重启...');
          } else if (result.upgraded && !result.restarted) {
            message.warning('更新完成，但服务重启失败，请手动重启');
          }
          // 重置版本状态，让用户可以重新检查
          setVersionStatus(null);
        } catch (e) {
          setUpgradeResult({
            upgraded: false,
            restarted: false,
            restartMessage: e instanceof Error ? e.message : '未知错误',
          });
          message.error('更新失败：' + (e instanceof Error ? e.message : '未知错误'));
        } finally {
          setUpgrading(false);
        }
      },
    });
  };

  return (
    <Spin spinning={versionLoading}>
      <Card title="NTD 版本信息" style={{ maxWidth: 600 }}>
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
                      message={
                        <Space>
                          <CheckCircleFilled style={{ color: '#52c41a' }} />
                          当前已是最新版本 {versionStatus.current}
                        </Space>
                      }
                      showIcon
                    />
                  ) : versionStatus.isUpToDate === false ? (
                    <Alert
                      type="info"
                      message={
                        <Space direction="vertical" size={4}>
                          <span>
                            <CloseCircleFilled style={{ color: '#1677ff', marginRight: 6 }} />
                            发现新版本：<strong>{versionStatus.latest}</strong>
                          </span>
                          <span style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
                            当前版本：{versionStatus.current}
                          </span>
                        </Space>
                      }
                      showIcon
                    />
                  ) : null}
                </div>
              )}

              {/* 升级结果展示 */}
              {upgradeResult && (
                <div style={{ marginBottom: 12 }}>
                  {upgradeResult.upgraded && upgradeResult.restarted ? (
                    <Alert
                      type="success"
                      message={
                        <Space direction="vertical" size={4}>
                          <span>
                            <CheckCircleFilled style={{ color: '#52c41a', marginRight: 6 }} />
                            更新命令执行成功，服务正在重启
                          </span>
                          {upgradeResult.npmOutput && (
                            <code style={{ fontSize: 11, color: 'var(--color-text-secondary)', display: 'block', marginTop: 4 }}>
                              {upgradeResult.npmOutput}
                            </code>
                          )}
                        </Space>
                      }
                      showIcon
                    />
                  ) : (
                    <Alert
                      type="warning"
                      message={
                        <Space direction="vertical" size={4}>
                          <span>
                            <ExclamationCircleFilled style={{ color: '#faad14', marginRight: 6 }} />
                            更新完成，但服务重启失败
                          </span>
                          {upgradeResult.npmOutput && (
                            <code style={{ fontSize: 11, color: 'var(--color-text-secondary)', display: 'block', marginTop: 4 }}>
                              {upgradeResult.npmOutput}
                            </code>
                          )}
                          {upgradeResult.restartMessage && (
                            <span style={{ fontSize: 12 }}>{upgradeResult.restartMessage}</span>
                          )}
                        </Space>
                      }
                      showIcon
                    />
                  )}
                </div>
              )}

              <Space>
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
  );
}
