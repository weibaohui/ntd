import { useState, useEffect, useRef } from 'react';
import { Card, InputNumber, Tooltip, Button, Popconfirm, Table, Empty, Switch, message, Form } from 'antd';
import { InfoCircleOutlined, SaveOutlined, StopOutlined, ReloadOutlined } from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import * as db from '@/utils/database';
import type { ExecutionRecord } from '@/types';

import { DEFAULT_EXECUTION_TIMEOUT_SECS, MAX_EXECUTION_TIMEOUT_MINUTES } from '@/constants';

/** 运行管理面板，负责展示执行并发、超时配置以及运行中任务操作。 */
export function RuntimePanel({ configForm, configSaving, handleSaveConfig, executorDisplayNames }: {
  configForm: any;
  configSaving: boolean;
  handleSaveConfig: () => Promise<void>;
  executorDisplayNames: Record<string, string>;
}) {
  const { state } = useApp();
  const { todos } = state;
  const [selectedRecordIds, setSelectedRecordIds] = useState<number[]>([]);
  const [stoppingRecords, setStoppingRecords] = useState(false);
  const [runningRecords, setRunningRecords] = useState<ExecutionRecord[]>([]);

  // 使用 Form.useWatch 订阅表单字段，直接响应 setFieldsValue 变化，
  // 解决 async db.getConfig() 完成后的 setFieldsValue 不会触发 useEffect deps 的问题。
  const watchedTimeoutSecs = Form.useWatch('execution_timeout_secs', configForm);

  // executionTimeoutSecs 由 useWatch 驱动，undefined 时使用懒初始化默认值。
  const [executionTimeoutSecs, setExecutionTimeoutSecs] = useState<number>(() =>
    watchedTimeoutSecs ?? DEFAULT_EXECUTION_TIMEOUT_SECS
  );

  // 同步 useWatch 值到本地 state（仅在值真正变化时更新）。
  // lastEnabledRef 同步记录非零值，供 toggle 恢复。
  const lastEnabledExecutionTimeoutSecsRef = useRef<number>(DEFAULT_EXECUTION_TIMEOUT_SECS);
  useEffect(() => {
    if (watchedTimeoutSecs !== undefined && watchedTimeoutSecs !== executionTimeoutSecs) {
      setExecutionTimeoutSecs(watchedTimeoutSecs);
      if (watchedTimeoutSecs !== 0) {
        lastEnabledExecutionTimeoutSecsRef.current = watchedTimeoutSecs;
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchedTimeoutSecs]);

  // 0 表示禁用执行超时（与后端 handlers/config.rs 中的校验对齐），其余值至少为 60 秒
  const executionTimeoutEnabled = executionTimeoutSecs !== 0;
  const executionTimeoutMinutes = executionTimeoutEnabled
    ? Math.max(1, Math.round(executionTimeoutSecs / 60))
    : undefined;

  /** 加载当前运行中的执行记录。 */
  const loadRunningRecords = async () => {
    try {
      const records = await db.getRunningExecutionRecords();
      setRunningRecords(records);
    } catch (err) {
      console.error('Failed to load running records:', err); // 中文：加载运行中任务失败
    }
  };

  useEffect(() => {
    loadRunningRecords();
    const timer = setInterval(loadRunningRecords, 10000);
    return () => clearInterval(timer);
  }, []);

  /** 批量停止当前选中的执行记录。 */
  const handleBatchStop = async () => {
    if (selectedRecordIds.length === 0) return;
    setStoppingRecords(true);
    const results = await Promise.allSettled(
      selectedRecordIds.map(async (recordId) => {
        await db.forceFailExecution(recordId);
      })
    );
    const successCount = results.filter(r => r.status === 'fulfilled').length;
    const failCount = results.filter(r => r.status === 'rejected').length;
    setSelectedRecordIds([]);
    setStoppingRecords(false);
    if (successCount > 0) message.success(`已停止 ${successCount} 个任务`);
    if (failCount > 0) message.error(`${failCount} 个任务停止失败`);
    loadRunningRecords();
  };

  /** 切换是否启用执行超时控制。 */
  const handleExecutionTimeoutToggle = (checked: boolean) => {
    if (!checked) {
      // 关闭时记录当前非零值，供后续重新开启时恢复。
      lastEnabledExecutionTimeoutSecsRef.current = executionTimeoutSecs;
    }
    const next = checked ? lastEnabledExecutionTimeoutSecsRef.current : 0;
    configForm.setFieldsValue({ execution_timeout_secs: next });
    // setExecutionTimeoutSecs 不需要：useWatch 会通过 setFieldsValue 触发，
    // useEffect [watchedTimeoutSecs] 会同步到本地 state。
  };

  return (
    <div style={{ padding: '8px 0' }}>
      <Card
        size="small"
        title="运行配置"
        style={{ marginBottom: 16 }}
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
              aria-label="执行超时开关"
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
                  configForm.setFieldsValue({ execution_timeout_secs: v * 60 });
                  // lastEnabledRef 由 useEffect [watchedTimeoutSecs] 集中更新
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

      <div style={{ marginBottom: 12, display: 'flex', alignItems: 'center', gap: 8 }}>
        <Button
          danger
          size="small"
          icon={<StopOutlined />}
          disabled={selectedRecordIds.length === 0}
          loading={stoppingRecords}
          onClick={handleBatchStop}
        >
          批量停止 ({selectedRecordIds.length})
        </Button>
        <Button
          size="small"
          icon={<ReloadOutlined />}
          onClick={loadRunningRecords}
        >
          刷新
        </Button>
        <span style={{ color: 'var(--color-text-secondary)', fontSize: 12 }}>
          共 {runningRecords.length} 个运行中任务
        </span>
      </div>
      <Table
        size="small"
        rowKey="id"
        dataSource={runningRecords}
        rowSelection={{
          selectedRowKeys: selectedRecordIds,
          onChange: (keys) => setSelectedRecordIds(keys as number[]),
        }}
        pagination={false}
        columns={[
          {
            title: 'Todo',
            key: 'todo_title',
            ellipsis: true,
            render: (_: unknown, record: ExecutionRecord) => {
              const todo = todos.find(t => t.id === record.todo_id);
              return todo ? todo.title : `#${record.todo_id}`;
            },
          },
          {
            title: '执行器',
            dataIndex: 'executor',
            key: 'executor',
            width: 110,
            render: (v: string | null) => {
              return executorDisplayNames[v || ''] || v || '-';
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
              <Popconfirm title="确认停止此任务？" onConfirm={async () => {
                try {
                  await db.forceFailExecution(record.id);
                  message.success('已停止');
                  loadRunningRecords();
                } catch (err) { message.error(`停止失败: ${err instanceof Error ? err.message : String(err)}`); }
              }}>
                <Button size="small" danger icon={<StopOutlined />} />
              </Popconfirm>
            ),
          },
        ]}
        locale={{ emptyText: <Empty description="暂无运行中任务" image={Empty.PRESENTED_IMAGE_SIMPLE} /> }}
      />
    </div>
  );
}
