import { useState, useEffect, useRef } from 'react';
import { Button, Card, Input, Switch, Spin, Tooltip, Modal, message, Typography, InputNumber, Form, Table, Space, Empty } from 'antd';
import { SearchOutlined, PlayCircleOutlined, ClockCircleOutlined, BugOutlined, CodeOutlined, InfoCircleOutlined, SaveOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CronPresetSelect } from '@/components/CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '@/utils/cron';
import { PageCard } from '@/components/common/PageCard';
import * as db from '@/utils/database';
import type { ExecutorConfig } from '@/types';

import { DEFAULT_EXECUTION_TIMEOUT_SECS, MAX_EXECUTION_TIMEOUT_MINUTES } from '@/constants';

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

  // 运行配置：并发数、超时等
  const [configForm] = Form.useForm();
  const [configSaving, setConfigSaving] = useState(false);
  const [executionTimeoutSecs, setExecutionTimeoutSecs] = useState<number>(() => DEFAULT_EXECUTION_TIMEOUT_SECS);
  const lastEnabledExecutionTimeoutSecsRef = useRef<number>(DEFAULT_EXECUTION_TIMEOUT_SECS);

  // 使用 Form.useWatch 订阅表单字段，直接响应 setFieldsValue 变化。
  const watchedTimeoutSecs = Form.useWatch('execution_timeout_secs', configForm);

  // 同步 watch 值到本地 state（仅处理外部 setFieldsValue 调用，如后端配置加载）。
  useEffect(() => {
    if (watchedTimeoutSecs === undefined) return;
    if (watchedTimeoutSecs !== executionTimeoutSecs) {
      setExecutionTimeoutSecs(watchedTimeoutSecs);
    }
    if (watchedTimeoutSecs !== 0) {
      lastEnabledExecutionTimeoutSecsRef.current = watchedTimeoutSecs;
    }
  }, [watchedTimeoutSecs, executionTimeoutSecs]);

  // 0 表示禁用执行超时，其余值至少为 60 秒
  const executionTimeoutEnabled = executionTimeoutSecs !== 0;
  const executionTimeoutMinutes = executionTimeoutEnabled
    ? Math.max(1, Math.round(executionTimeoutSecs / 60))
    : undefined;

  // Usage stats settings
  const [usageStatsEnabled, setUsageStatsEnabled] = useState(false);
  const [usageStatsCron, setUsageStatsCron] = useState('0 0 1 * * *');
  const [usageStatsLoading, setUsageStatsLoading] = useState(false);
  const [usageStatsSaving, setUsageStatsSaving] = useState(false);

  useEffect(() => {
    loadExecutors();
    loadConfig();
    loadUsageStatsSettings();
  }, []);

  /** 加载应用配置（并发数、超时等）。 */
  const loadConfig = async () => {
    try {
      const cfg = await db.getConfig();
      configForm.setFieldsValue(cfg);
    } catch {
      // 加载失败时使用默认值
    }
  };

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

  /**
   * 保存运行配置（并发数、超时等）。
   */
  const handleSaveConfig = async () => {
    try {
      const values = await configForm.validateFields();
      setConfigSaving(true);
      await db.updateConfig(values);
      message.success('配置已保存');
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setConfigSaving(false);
    }
  };

  /**
   * 切换是否启用执行超时控制。
   */
  const handleExecutionTimeoutToggle = (checked: boolean) => {
    if (!checked) {
      // 关闭时记录当前非零值，供后续重新开启时恢复。
      lastEnabledExecutionTimeoutSecsRef.current = executionTimeoutSecs;
    }
    const next = checked ? lastEnabledExecutionTimeoutSecsRef.current : 0;
    // 直接更新本地 state，确保 Switch 立即响应；
    // 同时调用 setFieldsValue 同步到表单供保存时读取。
    setExecutionTimeoutSecs(next);
    configForm.setFieldsValue({ execution_timeout_secs: next });
  };

  return (
    <PageCard icon={<CodeOutlined />} title="执行器">
      <Spin spinning={executorsLoading}>
        <div style={{ maxWidth: 1000 }}>
        <Paragraph type="secondary" style={{ marginBottom: 16 }}>
          管理执行器的路径、开关状态，并检测二进制是否可用。关闭开关的执行器不会出现在 Todo 的执行器选择列表中。
        </Paragraph>
        <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span style={{ color: 'var(--color-text-secondary)', fontSize: 13 }}>
            共 {executors.length} 个执行器
          </span>
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

        <Table
          rowKey="name"
          dataSource={executors}
          pagination={false}
          size="middle"
          locale={{ emptyText: <Empty description="暂无执行器" image={Empty.PRESENTED_IMAGE_SIMPLE} /> }}
          columns={[
            {
              title: '状态',
              dataIndex: 'enabled',
              key: 'enabled',
              width: 70,
              align: 'center',
              render: (enabled: boolean, record: ExecutorConfig) => (
                <Switch
                  size="small"
                  checked={enabled}
                  loading={savingExecutor === record.name}
                  onChange={async (checked) => {
                    setSavingExecutor(record.name);
                    try {
                      const updated = await db.updateExecutor(record.name, { enabled: checked });
                      setExecutors((prev) => prev.map((e) => e.name === record.name ? updated : e));
                    } catch (err: any) {
                      message.error('更新失败: ' + (err?.message || String(err)));
                    } finally {
                      setSavingExecutor(null);
                    }
                  }}
                />
              ),
            },
            {
              title: '执行器',
              dataIndex: 'display_name',
              key: 'display_name',
              width: 110,
              render: (name: string, record: ExecutorConfig) => (
                <span style={{ fontWeight: 500, opacity: record.enabled ? 1 : 0.5 }}>{name}</span>
              ),
            },
            {
              title: '二进制路径',
              dataIndex: 'path',
              key: 'path',
              render: (path: string, record: ExecutorConfig) => (
                <Input
                  size="small"
                  placeholder="二进制路径或命令名"
                  defaultValue={path}
                  onBlur={async (e) => {
                    const newPath = e.target.value.trim();
                    if (newPath === path) return;
                    setSavingExecutor(record.name);
                    try {
                      const updated = await db.updateExecutor(record.name, { path: newPath });
                      setExecutors((prev) => prev.map((ex) => ex.name === record.name ? updated : ex));
                      setDetectResults((prev) => {
                        const next = { ...prev };
                        delete next[record.name];
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
              ),
            },
            {
              title: 'Session 目录',
              dataIndex: 'session_dir',
              key: 'session_dir',
              width: 220,
              render: (sessionDir: string, record: ExecutorConfig) => (
                <Input
                  size="small"
                  placeholder="如 ~/.claude"
                  defaultValue={sessionDir}
                  onBlur={async (e) => {
                    const newDir = e.target.value.trim();
                    if (newDir === sessionDir) return;
                    setSavingExecutor(record.name);
                    try {
                      const updated = await db.updateExecutor(record.name, { session_dir: newDir });
                      setExecutors((prev) => prev.map((ex) => ex.name === record.name ? updated : ex));
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
              ),
            },
            {
              title: '检测状态',
              key: 'detect_status',
              width: 90,
              align: 'center',
              render: (_: unknown, record: ExecutorConfig) => {
                const detectResult = detectResults[record.name];
                if (!detectResult) {
                  return <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>未检测</span>;
                }
                return (
                  <Tooltip title={detectResult.resolved || '未找到'}>
                    {detectResult.found ? (
                      <span style={{ color: '#52c41a', fontSize: 12, fontWeight: 500 }}>
                        ✓ 可用
                      </span>
                    ) : (
                      <span style={{ color: '#ff4d4f', fontSize: 12, fontWeight: 500 }}>
                        ✗ 不可用
                      </span>
                    )}
                  </Tooltip>
                );
              },
            },
            {
              title: '操作',
              key: 'action',
              width: 180,
              render: (_: unknown, record: ExecutorConfig) => {
                const detectResult = detectResults[record.name];
                const isDetecting = detectingExecutor === record.name;
                const isTesting = testingExecutor === record.name;
                return (
                  <Space size={4}>
                    <Button
                      size="small"
                      icon={<SearchOutlined />}
                      loading={isDetecting}
                      onClick={async () => {
                        setDetectingExecutor(record.name);
                        try {
                          const result = await db.detectExecutor(record.name);
                          setDetectResults((prev) => ({ ...prev, [record.name]: { found: result.binary_found, resolved: result.path_resolved } }));
                          if (result.binary_found) {
                            message.success(`${record.display_name}: 找到 (${result.path_resolved})`);
                          } else {
                            message.warning(`${record.display_name}: 未找到`);
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
                    {!detectResult?.found && (
                      <Button
                        size="small"
                        icon={<BugOutlined />}
                        onClick={async () => {
                          try {
                            const result = await db.repairExecutor(record.name);
                            if (result.binary_found) {
                              setDetectResults((prev) => ({ ...prev, [record.name]: { found: true, resolved: result.path_resolved! } }));
                              const updated = await db.updateExecutor(record.name, { path: result.path_resolved!, enabled: true });
                              setExecutors((prev) => prev.map((e) => e.name === record.name ? updated : e));
                              if (result.path_updated) {
                                message.success(`已修复：${record.display_name} 路径更新为 ${result.path_resolved}`);
                              } else {
                                message.info(`路径已是最新：${result.path_resolved}`);
                              }
                            } else {
                              message.error(`未找到 ${record.display_name}，请手动填写路径`);
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
                        setTestingExecutor(record.name);
                        try {
                          const result = await db.testExecutor(record.name);
                          setTestModalData({ name: record.name, result });
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
                  </Space>
                );
              },
            },
          ]}
        />

        {/* 运行配置区域 */}
        <Card
          size="small"
          title={<><PlayCircleOutlined style={{ marginRight: 6 }} />运行配置</>}
          style={{ marginTop: 16 }}
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
              <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>执行超时</span>
              <Switch
                size="small"
                checked={executionTimeoutEnabled}
                checkedChildren="开启"
                unCheckedChildren="关闭"
                onChange={handleExecutionTimeoutToggle}
              />
              <InputNumber
                size="small"
                min={1}
                max={MAX_EXECUTION_TIMEOUT_MINUTES}
                style={{ width: 80 }}
                disabled={!executionTimeoutEnabled}
                value={executionTimeoutMinutes}
                onChange={(v) => {
                  if (v) {
                    const nextSecs = v * 60;
                    setExecutionTimeoutSecs(nextSecs);
                    configForm.setFieldsValue({ execution_timeout_secs: nextSecs });
                    lastEnabledExecutionTimeoutSecsRef.current = nextSecs;
                  }
                }}
              />
              <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>分钟</span>
              <Tooltip title={`单个执行任务的最大时长（1 ~ ${MAX_EXECUTION_TIMEOUT_MINUTES} 分钟，上限 7 天）；关闭后不再因超时自动终止`}>
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
