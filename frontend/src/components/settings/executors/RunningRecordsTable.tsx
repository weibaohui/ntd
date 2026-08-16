/**
 * RunningRecordsTable — 执行器面板「正在运行」tab（096-W4-4-3 产物）。
 *
 * 从 ExecutorsPanel 拆出的第二块表格：当前 workspace 下运行中的执行记录列表 + 批量停止。
 * 数据族由 useRunningRecords 托管，本组件只承接渲染与行交互（勾选/单行停止/刷新）。
 * 与 ExecutorsTable 对称：一个管「执行器配置」，一个管「正在运行的执行」。
 */

import { Button, Table, Empty, Popconfirm } from 'antd';
import { StopOutlined, ReloadOutlined } from '@ant-design/icons';
import type { ExecutionRecord } from '@/types';
import type { UseRunningRecordsReturn } from '@/hooks/useRunningRecords';

// 触发方式字面量→中文映射：提到模块级避免每次 render 重建字典。
// butler_chat 为 108 起现行值；default_response 仅存量记录展示用。
const triggerTypeMap: Record<string, string> = {
  manual: '手动',
  slash_command: '斜杠命令',
  butler_chat: '群聊管家',
  dm_chat: '单聊对话',
  default_response: '默认响应',
  scheduler: '定时',
};

export function RunningRecordsTable({ running }: { running: UseRunningRecordsReturn }) {
  return (
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
        <Button size="small" icon={<ReloadOutlined />} onClick={() => { void running.loadRunningRecords(); }}>
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
            render: (v: string | null) => running.executorDisplayNames[v || ''] || v || '-',
          },
          {
            title: '触发方式',
            dataIndex: 'trigger_type',
            key: 'trigger_type',
            width: 100,
            render: (v: string) => triggerTypeMap[v] || v,
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
  );
}
