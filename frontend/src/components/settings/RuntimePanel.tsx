import { useState, useEffect, useLayoutEffect, useRef } from 'react';
import { Card, InputNumber, Tooltip, Button, Popconfirm, Table, Empty, Switch, message } from 'antd';
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
  // 使用懒初始化避免表单未填充时读到默认值：初始化时若表单已有值则用表单的，否则用常量默认值。
  // 后续通过 useEffect 同步表单值变化，保持 UI 与表单状态一致。
  const [executionTimeoutSecs, setExecutionTimeoutSecs] = useState<number>(() =>
    configForm.getFieldValue('execution_timeout_secs') ?? DEFAULT_EXECUTION_TIMEOUT_SECS
  );
  // 0 表示禁用执行超时（与后端 handlers/config.rs 中的校验对齐），其余值至少为 60 秒
  const executionTimeoutEnabled = executionTimeoutSecs !== 0;
  const executionTimeoutMinutes = executionTimeoutEnabled
    ? Math.max(1, Math.round(executionTimeoutSecs / 60))
    : undefined;
  // 关闭时记录当前值，重新开启时恢复；避免再次开启时跳回初始默认值 3600。
  // 注意：仅在用户主动输入时更新，不在表单加载时覆盖（避免 0 值覆盖用户上次的非零设置）。
  const lastEnabledExecutionTimeoutSecsRef = useRef<number>(DEFAULT_EXECUTION_TIMEOUT_SECS);

  // 初始化 ref：若表单已有非零值则以该值填充，确保开关恢复时用正确的用户设置而非 3600。
  useLayoutEffect(() => {
    const formValue = configForm.getFieldValue('execution_timeout_secs');
    if (formValue !== undefined && formValue !== 0) {
      lastEnabledExecutionTimeoutSecsRef.current = formValue;
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // 仅首次渲染执行，不响应后续变更

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

  // 监听表单字段变化（外部加载配置重置表单时），同步本地状态。
  // 依赖 [configForm, executionTimeoutSecs]：当 async setFieldsValue 触发 re-render 后，
  // effect 会重新执行，此时 formValue 已更新而 executionTimeoutSecs 仍是旧值，触发同步。
  // formValue !== undefined 保护：忽略表单未填充时的 undefined，避免将 0 同步进去。
  useEffect(() => {
    const formValue = configForm.getFieldValue('execution_timeout_secs');
    if (formValue !== undefined && formValue !== executionTimeoutSecs) {
      setExecutionTimeoutSecs(formValue);
    }
  }, [configForm, executionTimeoutSecs]);

  /** 批量停止当前选中的执行任务。 */
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
      // 关闭前先记录当前非零值，供后续重新开启时恢复
      lastEnabledExecutionTimeoutSecsRef.current = executionTimeoutSecs;
    }
    const nextExecutionTimeoutSecs = checked ? lastEnabledExecutionTimeoutSecsRef.current : 0;
    setExecutionTimeoutSecs(nextExecutionTimeoutSecs);
    configForm.setFieldsValue({
      execution_timeout_secs: nextExecutionTimeoutSecs,
    });
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
                  const secs = v * 60;
                  setExecutionTimeoutSecs(secs);
                  configForm.setFieldsValue({ execution_timeout_secs: secs });
                  // 用户主动输入新值时同步更新 lastEnabledRef，供开关恢复
                  lastEnabledExecutionTimeoutSecsRef.current = secs;
                }
              }}
            />
            <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>分钟</span>
            <Tooltip title="单个执行任务的最大时长；关闭后不再因超时自动终止">
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
