import { Button, Input, Select, Switch, Spin, Tooltip, Modal, Typography, Table, Space, Empty, Tabs, Popconfirm } from 'antd';
import { SearchOutlined, PlayCircleOutlined, BugOutlined, CodeOutlined, StopOutlined, ReloadOutlined, StarOutlined, StarFilled } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { ProfilesPanel } from '@/components/settings/ProfilesPanel';
import { InstallExecutorButton } from '@/components/settings/InstallExecutorButton';
import { getExecutorInstallPrompt } from '@/components/settings/executorInstallPrompts';
import { RunConfigCard } from '@/components/settings/executors/RunConfigCard';
import { UsageStatsCard } from '@/components/settings/executors/UsageStatsCard';
import type { ExecutorConfig, ExecutionRecord } from '@/types';
import { SessionManager } from '@/components/SessionManager';
import { useExecutorAdmin } from '@/hooks/useExecutorAdmin';
import { useExecutorFieldSaver } from '@/hooks/useExecutorFieldSaver';
import { useRunningRecords } from '@/hooks/useRunningRecords';

const { Paragraph } = Typography;

/**
 * 执行器管理面板（096-W4-4-3 拆分后）：纯编排层。
 *
 * 原 942 行单体拆为 3 个 hook + 2 个卡片子组件，本组件只负责把它们接到 Tabs/Tables 的 JSX 上：
 * - useExecutorAdmin：列表 + 检测/测试/修复/设默认/安装刷新 + 模型缓存
 * - useExecutorFieldSaver：行内字段保存收敛（saveExecutorField + inlineFieldSave）
 * - useRunningRecords：运行监控族 + 面板 Tab 状态
 * - RunConfigCard / UsageStatsCard：运行配置 / AI 使用统计两块全局配置
 */
export function ExecutorsPanel() {
  // 三族状态分别由 hook 托管；saver 需要 admin 的 replaceExecutor 把保存结果回写列表。
  const admin = useExecutorAdmin();
  const saver = useExecutorFieldSaver(admin.replaceExecutor);
  // 运行监控族依赖执行器列表（派生 name→display_name 映射）。
  const running = useRunningRecords(admin.executors);

  return (
    <PageCard icon={<CodeOutlined />} title="执行器">
      <Tabs
        activeKey={running.runningTab}
        onChange={(key) => running.setRunningTab(key as 'executors' | 'running')}
        items={[
          {
            key: 'executors',
            label: '执行器',
            children: (
              <Spin spinning={admin.executorsLoading}>
                <div>
                  <Paragraph type="secondary" style={{ marginBottom: 16 }}>
                    管理执行器的路径、开关状态，并检测二进制是否可用。关闭开关的执行器不会出现在 Todo 的执行器选择列表中。
                  </Paragraph>
                  <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ color: 'var(--color-text-secondary)', fontSize: 13 }}>
                      共 {admin.executors.length} 个执行器
                    </span>
                    <Button
                      type="primary"
                      icon={<SearchOutlined />}
                      loading={admin.batchDetecting}
                      onClick={() => { void admin.batchDetect(); }}
                    >
                      批量检测
                    </Button>
                  </div>

                  <Table
                    rowKey="name"
                    dataSource={admin.executors}
                    pagination={false}
                    size="middle"
                    locale={{ emptyText: <Empty description="暂无执行器" image={Empty.PRESENTED_IMAGE_SIMPLE} /> }}
                    columns={[
                      {
                        title: '状态',
                        dataIndex: 'enabled',
                        key: 'enabled',
                        align: 'center',
                        render: (enabled: boolean, record: ExecutorConfig) => (
                          // enabled 开关属 onChange 型保存（非 blur），直接调 saveExecutorField。
                          <Switch
                            size="small"
                            checked={enabled}
                            loading={saver.savingExecutor === record.name}
                            onChange={(checked) => {
                              void saver.saveExecutorField(record.name, { enabled: checked });
                            }}
                          />
                        ),
                      },
                      {
                        title: '执行器',
                        dataIndex: 'display_name',
                        key: 'display_name',
                        render: (name: string, record: ExecutorConfig) => (
                          <span style={{ fontWeight: 500, opacity: record.enabled ? 1 : 0.5, display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                            {name}
                            {record.is_default && (
                              <Tooltip title="默认执行器">
                                <StarFilled style={{ color: '#faad14', fontSize: 12 }} />
                              </Tooltip>
                            )}
                          </span>
                        ),
                      },
                      {
                        title: '二进制路径',
                        dataIndex: 'path',
                        key: 'path',
                        render: (path: string, record: ExecutorConfig) => {
                          // Input 型行内保存：inlineFieldSave 封装 blur/Enter 双触发 + 未改不存；
                          // 改路径后顺手清检测状态（旧结果随路径失效）。
                          const save = saver.inlineFieldSave(record.name, path, async (newPath) => {
                            const updated = await saver.saveExecutorField(record.name, { path: newPath });
                            if (updated) admin.clearDetectResult(record.name);
                          });
                          return (
                            <Input
                              size="small"
                              placeholder="二进制路径或命令名"
                              defaultValue={path}
                              onBlur={save.onBlur}
                              onPressEnter={save.onPressEnter}
                            />
                          );
                        },
                      },
                      {
                        title: 'Session 目录',
                        dataIndex: 'session_dir',
                        key: 'session_dir',
                        render: (sessionDir: string, record: ExecutorConfig) => {
                          const save = saver.inlineFieldSave(record.name, sessionDir, async (newDir) => {
                            await saver.saveExecutorField(record.name, { session_dir: newDir });
                          });
                          return (
                            <Input
                              size="small"
                              placeholder="如 ~/.claude"
                              defaultValue={sessionDir}
                              onBlur={save.onBlur}
                              onPressEnter={save.onPressEnter}
                            />
                          );
                        },
                      },
                      {
                        // 默认模型：执行器级默认，所有未单独指定模型的 todo 用该执行器时默认传此模型。
                        // 留空 = 不传 --model，由执行器配置文件决定（向后兼容）。
                        title: '默认模型',
                        dataIndex: 'default_model',
                        key: 'default_model',
                        render: (defaultModel: string | null | undefined, record: ExecutorConfig) => {
                          // 已知能列模型的执行器（需和后端 list_models match 分支保持一致）。
                          if (!record.supports_models) {
                            const save = saver.inlineFieldSave(
                              record.name,
                              defaultModel ?? '',
                              async (newModel) => {
                                await saver.saveExecutorField(record.name, { default_model: newModel });
                              },
                            );
                            return (
                              <Input
                                size="small"
                                placeholder="留空用执行器自带配置"
                                defaultValue={defaultModel ?? ''}
                                onBlur={save.onBlur}
                                onPressEnter={save.onPressEnter}
                              />
                            );
                          }
                          // supports_models：Select 下拉，按 provider 分组展示后端列出的模型。
                          const models = admin.executorModels[record.name] || [];
                          const groups: Record<string, { label: string; value: string }[]> = {};
                          models.forEach((full) => {
                            const slash = full.indexOf('/');
                            const provider = slash > 0 ? full.slice(0, slash) : '其他';
                            const mn = slash > 0 ? full.slice(slash + 1) : full;
                            if (!groups[provider]) groups[provider] = [];
                            groups[provider].push({ label: mn, value: full });
                          });
                          return (
                            <Select
                              size="small"
                              value={defaultModel || undefined}
                              placeholder="留空用执行器自带配置"
                              allowClear
                              showSearch
                              notFoundContent={
                                admin.modelsLoading[record.name]
                                  ? '加载中，请稍后...'
                                  : models.length === 0
                                    ? '暂无可选模型'
                                    : undefined
                              }
                              onDropdownVisibleChange={(open) => admin.handleModelsDropdown(record.name, open)}
                              filterOption={(input: string, option?: { label: string; value: string }) =>
                                (option?.label ?? '').toLowerCase().includes(input.toLowerCase())}
                              onChange={(v: unknown) => {
                                const newModel = (v as string)?.trim() || '';
                                // 未改动不保存（避免清空即触发无谓请求）。
                                if (newModel === (defaultModel ?? '')) return;
                                void saver.saveExecutorField(record.name, { default_model: newModel });
                              }}
                              style={{ width: '100%' }}
                            >
                              {Object.entries(groups).map(([provider, items]) => (
                                <Select.OptGroup key={provider} label={provider}>
                                  {items.map((item) => (
                                    <Select.Option key={item.value} value={item.value}>{item.label}</Select.Option>
                                  ))}
                                </Select.OptGroup>
                              ))}
                            </Select>
                          );
                        },
                      },
                      {
                        title: '检测状态',
                        key: 'detect_status',
                        align: 'center',
                        render: (_: unknown, record: ExecutorConfig) => {
                          const detectResult = admin.detectResults[record.name];
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
                        width: 240,
                        render: (_: unknown, record: ExecutorConfig) => {
                          const detectResult = admin.detectResults[record.name];
                          const isDetecting = admin.detectingExecutor === record.name;
                          const isTesting = admin.testingExecutor === record.name;
                          const isSettingDefault = admin.settingDefaultExecutor === record.name;
                          return (
                            <Space size={4}>
                              <Tooltip title={record.is_default ? '当前为默认执行器' : '设为默认执行器'}>
                                <Button
                                  size="small"
                                  type={record.is_default ? 'primary' : 'default'}
                                  icon={record.is_default ? <StarFilled /> : <StarOutlined />}
                                  loading={isSettingDefault}
                                  disabled={record.is_default}
                                  onClick={() => { void admin.setAsDefault(record); }}
                                >
                                  {record.is_default ? '默认' : '设为默认'}
                                </Button>
                              </Tooltip>
                              <Button
                                size="small"
                                icon={<SearchOutlined />}
                                loading={isDetecting}
                                onClick={() => { void admin.detectExecutorByName(record); }}
                              >
                                检测
                              </Button>
                              {!detectResult?.found && (
                                <Button
                                  size="small"
                                  icon={<BugOutlined />}
                                  onClick={() => { void admin.repairByName(record); }}
                                >
                                  修复
                                </Button>
                              )}
                              {detectResult && !detectResult.found && (() => {
                                // 把 getExecutorInstallPrompt 结果存为临时变量，避免条件判断和 prompt prop 重复调用
                                const installPrompt = getExecutorInstallPrompt(record.name);
                                return installPrompt && (
                                  <InstallExecutorButton
                                    executorName={record.name}
                                    displayName={record.display_name}
                                    prompt={installPrompt.prompt}
                                    buttonSize="small"
                                    showLabel={true}
                                    // 安装完成后：重新检测 → 若找到则修复路径 + 启用 → 刷新前端状态
                                    //（detect+repair+updateExecutor 三步兜底链已收口到 refreshAfterInstall）。
                                    onInstalled={() => { void admin.refreshAfterInstall(record); }}
                                  />
                                );
                              })()}
                              <Button
                                size="small"
                                type="primary"
                                ghost
                                icon={<PlayCircleOutlined />}
                                loading={isTesting}
                                onClick={() => { void admin.testExecutorByName(record); }}
                              >
                                测试
                              </Button>
                            </Space>
                          );
                        },
                      },
                    ]}
                  />

                  {/* 运行配置 / AI 使用统计两块全局配置已拆为独立卡片子组件，各自挂载即加载。 */}
                  <RunConfigCard />
                  <UsageStatsCard />

                  {/* 执行器测试结果 Modal */}
                  <Modal
                    title={
                      admin.testModalData
                        ? `测试结果 - ${admin.executors.find((e) => e.name === admin.testModalData?.name)?.display_name || admin.testModalData?.name}`
                        : '测试结果'
                    }
                    open={admin.testModalVisible}
                    onCancel={admin.closeTestModal}
                    footer={<Button onClick={admin.closeTestModal}>关闭</Button>}
                    width={500}
                  >
                    {admin.testModalData && (
                      <div>
                        <p>
                          状态：{admin.testModalData.result.test_passed
                            ? <span style={{ color: '#52c41a', fontWeight: 600 }}>通过</span>
                            : <span style={{ color: '#ff4d4f', fontWeight: 600 }}>失败</span>
                          }
                        </p>
                        {admin.testModalData.result.error && (
                          <p style={{ color: '#ff4d4f' }}>错误：{admin.testModalData.result.error}</p>
                        )}
                        {admin.testModalData.result.output && (
                          <div>
                            <Paragraph type="secondary">输出：</Paragraph>
                            <pre style={{
                              background: 'var(--color-bg-container)',
                              color: 'var(--color-text-secondary)',
                              padding: 12,
                              borderRadius: 6,
                              fontSize: 12,
                              maxHeight: 300,
                              overflow: 'auto',
                              whiteSpace: 'pre-wrap',
                              margin: 0,
                            }}>
                              {admin.testModalData.result.output}
                            </pre>
                          </div>
                        )}
                      </div>
                    )}
                  </Modal>
                </div>
              </Spin>
            ),
          },
          {
            key: 'api-key',
            label: 'API Key',
            children: (
              <ProfilesPanel />
            ),
          },
          {
            key: 'running',
            label: '正在运行',
            children: (
              <div style={{ padding: '8px 0' }}>
                <div style={{ marginBottom: 12, display: 'flex', alignItems: 'center', gap: 8 }}>
                  <Button
                    danger
                    size="small"
                    icon={<StopOutlined />}
                    disabled={running.selectedRecordIds.length === 0}
                    loading={running.stoppingRecords}
                    onClick={() => { void running.handleBatchStop(); }}
                  >
                    批量停止 ({running.selectedRecordIds.length})
                  </Button>
                  <Button
                    size="small"
                    icon={<ReloadOutlined />}
                    onClick={() => { void running.loadRunningRecords(); }}
                  >
                    刷新
                  </Button>
                  <span style={{ color: 'var(--color-text-secondary)', fontSize: 12 }}>
                    共 {running.runningRecords.length} 个运行中任务
                  </span>
                </div>
                <Table
                  size="small"
                  rowKey="id"
                  dataSource={running.runningRecords}
                  rowSelection={{
                    selectedRowKeys: running.selectedRecordIds,
                    onChange: (keys) => running.setSelectedRecordIds(keys as number[]),
                  }}
                  pagination={false}
                  columns={[
                    {
                      title: 'Todo',
                      key: 'todo_title',
                      ellipsis: true,
                      render: (_: unknown, record: ExecutionRecord) => {
                        const todo = running.recordTodos.find((t) => t.id === record.todo_id);
                        return todo ? todo.title : `#${record.todo_id}`;
                      },
                    },
                    {
                      title: '执行器',
                      dataIndex: 'executor',
                      key: 'executor',
                      width: 110,
                      render: (v: string | null) => {
                        return running.executorDisplayNames[v || ''] || v || '-';
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
                        <Popconfirm title="确认停止此任务？" onConfirm={() => { void running.stopRecord(record.id); }}>
                          <Button type="text" size="small" icon={<StopOutlined />} />
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
            key: 'sessions',
            label: '会话',
            children: (
              <SessionManager embedded />
            ),
          },
        ]}
      />
    </PageCard>
  );
}
