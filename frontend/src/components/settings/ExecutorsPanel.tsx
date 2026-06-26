import { useState, useEffect } from 'react';
import { Button, Card, Input, Switch, Spin, Tooltip, Modal, message, Typography } from 'antd';
import { SearchOutlined, PlayCircleOutlined, ClockCircleOutlined, BugOutlined, CodeOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CronPresetSelect } from '@/components/CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '@/utils/cron';
import { PageCard } from '@/components/common/PageCard';
import * as db from '@/utils/database';
import type { ExecutorConfig } from '@/types';

const { Paragraph } = Typography;

/** 执行器管理面板，负责展示和管理各执行器的配置与可用性检测。 */
export function ExecutorsPanel() {
  const [executors, setExecutors] = useState<ExecutorConfig[]>([]);
  const [executorsLoading, setExecutorsLoading] = useState(false);
  const [detectResults, setDetectResults] = useState<Record<string, { found: boolean; resolved: string | null }>>({});
  const [detectingExecutor, setDetectingExecutor] = useState<string | null>(null);
  const [testingExecutor, setTestingExecutor] = useState<string | null>(null);
  const [batchDetecting, setBatchDetecting] = useState(false);
  const [testModalVisible, setTestModalVisible] = useState(false);
  const [testModalData, setTestModalData] = useState<{ name: string; result: { test_passed: boolean; output: string | null; error: string | null } } | null>(null);
  const [savingExecutor, setSavingExecutor] = useState<string | null>(null);

  // Usage stats settings
  const [usageStatsEnabled, setUsageStatsEnabled] = useState(false);
  const [usageStatsCron, setUsageStatsCron] = useState('0 0 1 * * *');
  const [usageStatsLoading, setUsageStatsLoading] = useState(false);
  const [usageStatsSaving, setUsageStatsSaving] = useState(false);

  useEffect(() => {
    loadExecutors();
    loadUsageStatsSettings();
  }, []);

  /** 从数据库加载执行器配置列表。 */
  const loadExecutors = async () => {
    try {
      setExecutorsLoading(true);
      const list = await db.getExecutors();
      setExecutors(list);
    } catch (err: any) {
      message.error('加载执行器配置失败: ' + (err?.message || String(err)));
    } finally {
      setExecutorsLoading(false);
    }
  };

  const loadUsageStatsSettings = async () => {
    try {
      setUsageStatsLoading(true);
      const settings = await db.getUsageStatsSettings();
      setUsageStatsEnabled(settings.auto_usage_stats_enabled);
      setUsageStatsCron(settings.auto_usage_stats_cron);
    } catch {
      // Ignore errors, use defaults
    } finally {
      setUsageStatsLoading(false);
    }
  };

  const handleSaveUsageStats = async () => {
    try {
      setUsageStatsSaving(true);
      await db.updateUsageStatsSettings(usageStatsEnabled, usageStatsCron);
      message.success('AI 使用统计配置已更新');
    } catch (err: any) {
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setUsageStatsSaving(false);
    }
  };

  return (
    <PageCard icon={<CodeOutlined />} title="执行器">
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
                    setDetectResults((prev) => ({
                      ...prev,
                      [ec.name]: { found: result.binary_found, resolved: result.path_resolved },
                    }));
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
                  {/* 修复按钮：路径无效时尝试用 which 查找真实路径并自动更新 */}
                  {!detectResult?.found && (
                    <Button
                      size="small"
                      icon={<BugOutlined />}
                      onClick={async () => {
                        try {
                          const result = await db.repairExecutor(ec.name);
                          if (result.binary_found) {
                            setDetectResults((prev) => ({ ...prev, [ec.name]: { found: true, resolved: result.path_resolved! } }));
                            const updated = await db.updateExecutor(ec.name, { path: result.path_resolved!, enabled: true });
                            setExecutors((prev) => prev.map((e) => e.name === ec.name ? updated : e));
                            if (result.path_updated) {
                              message.success(`已修复：${ec.display_name} 路径更新为 ${result.path_resolved}`);
                            } else {
                              message.info(`路径已是最新：${result.path_resolved}`);
                            }
                          } else {
                            message.error(`未找到 ${ec.display_name}，请手动填写路径`);
                          }
                        } catch (err: any) {
                          message.error('修复失败: ' + (err?.message || String(err)));
                        }
                      }}
                    >
                      修复
                    </Button>
                  )}
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

        <Card
          size="small"
          title={<><ClockCircleOutlined style={{ marginRight: 6 }} />AI 使用统计</>}
          style={{ marginTop: 16 }}
          extra={
            <Switch
              checked={usageStatsEnabled}
              onChange={async (checked) => {
                setUsageStatsEnabled(checked);
                try {
                  setUsageStatsSaving(true);
                  await db.updateUsageStatsSettings(checked, usageStatsCron);
                  message.success('AI 使用统计配置已更新');
                } catch (err: any) {
                  message.error('保存失败: ' + (err?.message || String(err)));
                } finally {
                  setUsageStatsSaving(false);
                }
              }}
              loading={usageStatsLoading}
            />
          }
        >
          {usageStatsEnabled && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <Typography.Paragraph type="secondary" style={{ marginBottom: 8 }}>
                自动收集本机执行器的 Token 使用量，每日归档到数据库
              </Typography.Paragraph>
              <CronPresetSelect value={usageStatsCron} onChange={(val: string) => setUsageStatsCron(val)} />
              <Cron
                value={cronTo5(usageStatsCron)}
                setValue={(val: string) => { setUsageStatsCron(cronTo6(val)); }}
                locale={CRON_ZH_LOCALE}
                defaultPeriod="day"
                humanizeLabels
                allowClear={false}
              />
              <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
                <Button size="small" type="primary" onClick={handleSaveUsageStats} loading={usageStatsSaving}>
                  保存
                </Button>
              </div>
            </div>
          )}
        </Card>
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
    </PageCard>
  );
}
