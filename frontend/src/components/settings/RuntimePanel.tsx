import { useState, useEffect } from 'react';
import { Card, InputNumber, Tooltip, Button, Popconfirm, Table, Empty, message } from 'antd';
import { InfoCircleOutlined, SaveOutlined, StopOutlined, ReloadOutlined } from '@ant-design/icons';
import { useApp } from '../../hooks/useApp';
import * as db from '../../utils/database';
import type { ExecutionRecord } from '../../types';

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

  const loadRunningRecords = async () => {
    try {
      const records = await db.getRunningExecutionRecords();
      setRunningRecords(records);
    } catch (err) {
      console.error('Failed to load running records:', err);
    }
  };

  useEffect(() => {
    loadRunningRecords();
    const timer = setInterval(loadRunningRecords, 10000);
    return () => clearInterval(timer);
  }, []);

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
            <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>超时时间(分钟)</span>
            <InputNumber
              size="small"
              min={1}
              max={1440}
              style={{ width: 80 }}
              value={Math.round((configForm.getFieldValue('execution_timeout_secs') ?? 1800) / 60)}
              onChange={(v) => {
                if (v) {
                  configForm.setFieldsValue({ execution_timeout_secs: v * 60 });
                }
              }}
            />
            <Tooltip title="单个对话执行的最大时长，超时将自动终止">
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
